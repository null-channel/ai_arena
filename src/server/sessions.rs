use super::{
    error::AppError,
    state::{AppState, ArenaMode},
};
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
use std::sync::Arc;
use tokio::sync::mpsc;

// Game session tracking
pub struct GameSession {
    pub(super) mode: ArenaMode,
    pub game_type: String,
    pub game: Game,
    pub agents: Vec<AIAgentConfig>,
    pub status: GameStatus,
    pub events: Arc<Vec<GameEvent>>,
    pub result: Option<TestResult>,
}

impl GameSession {
    pub(super) fn pending(game: Game, agents: Vec<AIAgentConfig>, mode: ArenaMode) -> Self {
        Self {
            mode,
            game_type: game.name().to_owned(),
            game,
            agents,
            status: GameStatus::Pending,
            events: Arc::default(),
            result: None,
        }
    }

    pub(super) fn record_event(&mut self, event: GameEvent) {
        if self.result.is_some() {
            self.events = Arc::default();
        }
        Arc::make_mut(&mut self.events).push(event);
    }

    pub(super) fn finish(&mut self, result: TestResult) {
        self.status = if result.error().is_some() {
            GameStatus::Error
        } else {
            GameStatus::Completed
        };
        self.result = Some(result);
        // The result now supplies the battle log. Keep the last event for the JSON API.
        let events = Arc::make_mut(&mut self.events);
        events.drain(..events.len().saturating_sub(1));
    }
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
    pub(super) mode: ArenaMode,
    pub(super) game_id: String,
    pub(super) game_type: String,
    pub(super) agents: Vec<AIAgentConfig>,
    pub(super) status: GameStatus,
    pub(super) result: Option<TestResult>,
}

impl GameSummary {
    fn from_session(game_id: &str, session: &GameSession) -> Self {
        Self {
            mode: session.mode,
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
            mode: ArenaMode::Real,
            game_id: run.id,
            game_type: run.game_type,
            agents: run.agents,
            status: GameStatus::from_storage(&run.status),
            result: run.result,
        }
    }
}

pub(super) async fn live_game_summaries(state: &AppState) -> Vec<GameSummary> {
    let mut games = state
        .games
        .lock()
        .await
        .iter()
        .filter(|(_, session)| matches!(session.status, GameStatus::Pending | GameStatus::Running))
        .map(|(id, session)| GameSummary::from_session(id, session))
        .collect::<Vec<_>>();
    games.sort_by(|a, b| a.game_id.cmp(&b.game_id));
    games
}

pub(super) async fn game_detail(
    state: &AppState,
    game_id: &str,
) -> Result<(GameSummary, Arc<Vec<GameEvent>>), AppError> {
    if let Some(session) = state.games.lock().await.get(game_id) {
        return Ok((
            GameSummary::from_session(game_id, session),
            session.events.clone(),
        ));
    }
    let run = state
        .store
        .get_run(game_id)
        .await
        .map_err(AppError::Internal)?
        .ok_or_else(|| AppError::NotFound("Game not found".to_string()))?;
    Ok((run.into(), Arc::default()))
}

pub(super) async fn create_game_session(
    state: &AppState,
    game_type: String,
    agents: Vec<AIAgentConfig>,
) -> Result<String, AppError> {
    let game = Game::new(&game_type)
        .ok_or_else(|| AppError::BadRequest("Invalid game type".to_string()))?;
    let mode = state.mode();
    let game_id = state
        .result_store(mode)
        .create_run(&game, &agents, Some(RunMetadata::source("web")))
        .await
        .map_err(AppError::Internal)?;

    let session = GameSession::pending(game, agents, mode);
    state.games.lock().await.insert(game_id.clone(), session);
    Ok(game_id)
}

pub(super) async fn run_session(state: AppState, game_id: String) {
    let (game, agents, mode) = {
        let mut games = state.games.lock().await;
        let Some(session) = games.get_mut(&game_id) else {
            return;
        };

        if !matches!(session.status, GameStatus::Pending) {
            return;
        }

        session.status = GameStatus::Running;
        (session.game.clone(), session.agents.clone(), session.mode)
    };

    if let Err(error) = state.result_store(mode).mark_running(&game_id).await {
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
    let record_events = async {
        while let Some(event) = rx.recv().await {
            if let Some(session) = state.games.lock().await.get_mut(&game_id) {
                session.record_event(event);
            }
        }
    };

    let (final_result, ()) = tokio::join!(game_future, record_events);
    match &final_result {
        Ok(result) => {
            if let Err(error) = state
                .result_store(mode)
                .complete_run(&game_id, result)
                .await
            {
                eprintln!("Failed to store DuckDB run {}: {}", game_id, error);
            }
        }
        Err(_) => {
            if let Err(error) = state
                .result_store(mode)
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
            Ok(result) => session.finish(result),
            Err(_) => {
                session.status = GameStatus::Error;
                session.result = None;
            }
        }
    }
}
