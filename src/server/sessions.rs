use super::{error::AppError, state::AppState};
use crate::{
    agent_config::AIAgentConfig,
    games::{
        events::GameEvent,
        game::{Game, TestResult},
    },
    storage::{RunMetadata, StoredRun},
};
use futures_util::FutureExt;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

// Game session tracking
pub struct GameSession {
    pub game_type: String,
    pub game: Game,
    pub agents: Vec<AIAgentConfig>,
    pub status: GameStatus,
    pub latest_event: Option<GameEvent>,
    pub result: Option<TestResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GameStatus {
    Pending,
    Running,
    Completed,
    Error,
}

impl GameStatus {
    pub(super) fn from_storage(status: &str) -> Self {
        match status {
            "pending" => GameStatus::Pending,
            "running" => GameStatus::Running,
            "completed" => GameStatus::Completed,
            "error" => GameStatus::Error,
            _ => GameStatus::Error,
        }
    }
}

pub(super) struct GameSummary {
    pub(super) game_id: String,
    pub(super) game_type: String,
    pub(super) agents: Vec<AIAgentConfig>,
    pub(super) status: GameStatus,
    pub(super) result: Option<TestResult>,
}

impl GameSummary {
    fn from_session(game_id: &str, session: &GameSession) -> Self {
        Self {
            game_id: game_id.to_owned(),
            game_type: session.game_type.clone(),
            agents: session.agents.clone(),
            status: session.status.clone(),
            result: session.result.clone(),
        }
    }
}

impl From<StoredRun> for GameSummary {
    fn from(run: StoredRun) -> Self {
        Self {
            game_id: run.id,
            game_type: run.game_type,
            agents: run.agents,
            status: GameStatus::from_storage(&run.status),
            result: run.result,
        }
    }
}

pub(super) async fn game_summaries(state: &AppState) -> Vec<GameSummary> {
    let mut sessions = state
        .games
        .lock()
        .await
        .iter()
        .map(|(id, session)| (id.clone(), GameSummary::from_session(id, session)))
        .collect::<std::collections::BTreeMap<_, _>>();
    match state.store.list_runs().await {
        Ok(runs) => {
            for run in runs {
                sessions.entry(run.id.clone()).or_insert_with(|| run.into());
            }
        }
        Err(error) => eprintln!("Failed to list DuckDB runs: {}", error),
    }
    sessions.into_values().collect()
}

pub(super) async fn game_detail(
    state: &AppState,
    game_id: &str,
) -> Result<(GameSummary, Option<GameEvent>), AppError> {
    if let Some(session) = state.games.lock().await.get(game_id) {
        return Ok((
            GameSummary::from_session(game_id, session),
            session.latest_event.clone(),
        ));
    }
    let run = state
        .store
        .get_run(game_id)
        .await
        .map_err(AppError::Internal)?
        .ok_or_else(|| AppError::NotFound("Game not found".to_string()))?;
    Ok((run.into(), None))
}

pub(super) async fn create_game_session(
    state: &AppState,
    game_type: String,
    agents: Vec<AIAgentConfig>,
) -> Result<String, AppError> {
    create_game_session_with_metadata(state, game_type, agents, RunMetadata::source("web")).await
}

pub(super) async fn create_game_session_with_metadata(
    state: &AppState,
    game_type: String,
    agents: Vec<AIAgentConfig>,
    metadata: RunMetadata,
) -> Result<String, AppError> {
    let game = Game::new(&game_type)
        .ok_or_else(|| AppError::BadRequest("Invalid game type".to_string()))?;
    let game_id = state
        .store
        .create_run(&game, &agents, Some(metadata))
        .await
        .map_err(AppError::Internal)?;

    let session = GameSession {
        game_type,
        game,
        agents,
        status: GameStatus::Pending,
        latest_event: None,
        result: None,
    };

    state.games.lock().await.insert(game_id.clone(), session);
    Ok(game_id)
}

pub(super) async fn run_session(state: AppState, game_id: String) {
    let (game, agents) = {
        let mut games = state.games.lock().await;
        let Some(session) = games.get_mut(&game_id) else {
            return;
        };

        if !matches!(session.status, GameStatus::Pending) {
            return;
        }

        session.status = GameStatus::Running;
        (session.game.clone(), session.agents.clone())
    };

    if let Err(error) = state.store.mark_running(&game_id).await {
        eprintln!(
            "Failed to mark DuckDB run {} as running: {}",
            game_id, error
        );
    }

    let (tx, mut rx) = mpsc::channel::<GameEvent>(100);
    let secrets = {
        let secrets = state.secrets.lock().await;
        secrets.clone()
    };

    let game_future =
        std::panic::AssertUnwindSafe(game.play_game(agents, Some(tx), Some(&secrets)))
            .catch_unwind();
    tokio::pin!(game_future);
    let mut result = None;

    loop {
        tokio::select! {
            update = rx.recv() => {
                if let Some(update) = update {
                    let mut games = state.games.lock().await;
                    if let Some(session) = games.get_mut(&game_id) {
                        session.latest_event = Some(update);
                    }
                } else if result.is_some() {
                    break;
                }
            }
            completed = &mut game_future, if result.is_none() => {
                result = Some(completed);
                if rx.is_closed() {
                    break;
                }
            }
        }
    }

    let final_result = result.expect("game future completes before session finalization");
    match &final_result {
        Ok(result) => {
            if let Err(error) = state.store.complete_run(&game_id, result).await {
                eprintln!("Failed to store DuckDB run {}: {}", game_id, error);
            }
        }
        Err(_) => {
            if let Err(error) = state
                .store
                .mark_error(&game_id, "game execution panicked")
                .await
            {
                eprintln!("Failed to mark DuckDB run {} as error: {}", game_id, error);
            }
        }
    }

    let mut games = state.games.lock().await;
    if let Some(session) = games.get_mut(&game_id) {
        match final_result {
            Ok(result) => {
                session.status = if result.error().is_some() {
                    GameStatus::Error
                } else {
                    GameStatus::Completed
                };
                session.result = Some(result);
            }
            Err(_) => {
                session.status = GameStatus::Error;
                session.result = None;
            }
        }
    }
}
