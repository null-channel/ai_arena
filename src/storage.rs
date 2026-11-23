use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use duckdb::{Connection, params};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::agent_config::AIAgentConfig;
use crate::games::game::{Game, TestResult};
use crate::games::stats::TurnStats;

pub const DEFAULT_DB_PATH: &str = "ai_arena.duckdb";

#[derive(Clone, Debug)]
pub struct RunStore {
    path: Arc<PathBuf>,
    lock: Arc<Mutex<()>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StoredRun {
    pub id: String,
    pub game_type: String,
    pub status: String,
    pub agents: Vec<AIAgentConfig>,
    pub result: Option<TestResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matrix_job_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StoredTurn {
    pub sequence: u64,
    #[serde(flatten)]
    pub turn: TurnStats,
}

#[derive(Clone, Debug, Serialize)]
pub struct LeaderboardEntry {
    pub model: String,
    pub games: u32,
    pub wins: u32,
    pub losses: u32,
    pub draws: u32,
    pub invalid_moves: u32,
    pub average_turn_time_ms: f64,
}

#[derive(Default)]
struct LeaderboardAccumulator {
    games: u32,
    wins: u32,
    losses: u32,
    draws: u32,
    invalid_moves: u32,
    total_turn_time_ms: u64,
    turns: u32,
}

#[async_trait::async_trait]
pub trait GameStorage: Send + Sync {
    async fn create_run(
        &self,
        game: &Game,
        agents: &[AIAgentConfig],
        metadata: Option<RunMetadata>,
    ) -> Result<String, String>;
    async fn append_event(&self, run_id: &str, turn: &TurnStats) -> Result<(), String>;
    async fn finish_run(&self, run_id: &str, result: &TestResult) -> Result<(), String>;
    async fn get_run(&self, run_id: &str) -> Result<Option<StoredRun>, String>;
    async fn list_events(&self, run_id: &str) -> Result<Vec<StoredTurn>, String>;
}

#[derive(Clone, Debug, Serialize)]
pub struct RunMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repetition: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matrix_job_id: Option<String>,
}

impl RunMetadata {
    pub fn source(source: &str) -> Self {
        Self {
            source: Some(source.to_string()),
            description: None,
            repetition: None,
            matrix_job_id: None,
        }
    }
}

impl RunStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: Arc::new(path.into()),
            lock: Arc::new(Mutex::new(())),
        }
    }

    pub fn default_path() -> PathBuf {
        PathBuf::from(DEFAULT_DB_PATH)
    }

    pub async fn initialize(&self) -> Result<(), String> {
        self.with_connection(|conn| {
            ensure_schema(&conn)?;
            Ok(())
        })
        .await
    }

    pub async fn create_run(
        &self,
        game: &Game,
        agents: &[AIAgentConfig],
        metadata: Option<RunMetadata>,
    ) -> Result<String, String> {
        let run_id = Uuid::new_v4().to_string();
        let game_type = game.name().to_string();
        let agents_json = to_json_string(agents)?;
        let game_config_json = to_json_string(game)?;
        let matrix_job_id = metadata
            .as_ref()
            .and_then(|metadata| metadata.matrix_job_id.clone());
        let metadata_json = metadata
            .map(|metadata| to_json_string(&metadata))
            .transpose()?;
        let created_at = now_timestamp();
        let run_id_for_insert = run_id.clone();

        self.with_connection(move |conn| {
            ensure_schema(&conn)?;
            conn.execute(
                "INSERT INTO runs (
                    id, game_type, status, created_at, agents_json, game_config_json, matrix_job_id, metadata_json
                ) VALUES (?1, ?2, 'pending', ?3, ?4, ?5, ?6, ?7)",
                params![
                    run_id_for_insert,
                    game_type,
                    created_at,
                    agents_json,
                    game_config_json,
                    matrix_job_id,
                    metadata_json
                ],
            )
            .map_err(|err| err.to_string())?;
            Ok(())
        })
        .await?;

        Ok(run_id)
    }

    pub async fn mark_running(&self, run_id: &str) -> Result<(), String> {
        let run_id = run_id.to_string();
        let started_at = now_timestamp();
        self.with_connection(move |conn| {
            ensure_schema(&conn)?;
            conn.execute(
                "UPDATE runs SET status = 'running', started_at = COALESCE(started_at, ?2)
                 WHERE id = ?1",
                params![run_id, started_at],
            )
            .map_err(|err| err.to_string())?;
            Ok(())
        })
        .await
    }

    pub async fn complete_run(&self, run_id: &str, result: &TestResult) -> Result<(), String> {
        let run_id = run_id.to_string();
        let result = result.clone();
        let finished_at = now_timestamp();
        self.with_connection(move |conn| {
            ensure_schema(&conn)?;
            let stats = result.stats();
            let status = if result.error().is_some() {
                "error"
            } else {
                "completed"
            };
            let result_json = to_json_string(&result)?;
            let error = result.error().map(ToString::to_string);

            conn.execute("DELETE FROM run_turns WHERE run_id = ?1", params![run_id])
                .map_err(|err| err.to_string())?;

            for (seq, turn) in stats.turns.iter().enumerate() {
                conn.execute(
                    "INSERT INTO run_turns (
                        run_id, seq, turn_number, player, move_json, valid, error,
                        time_taken_ms, state_before_json, state_after_json, diagnostics
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                    params![
                        run_id,
                        seq as i64,
                        turn.turn_number as i64,
                        turn.player,
                        turn.move_made.to_string(),
                        turn.move_valid,
                        turn.error_message,
                        turn.time_taken_ms as i64,
                        turn.state_before.to_string(),
                        turn.state_after.to_string(),
                        turn.diagnostics
                    ],
                )
                .map_err(|err| err.to_string())?;
            }

            conn.execute(
                "UPDATE runs SET
                    status = ?2,
                    finished_at = ?3,
                    winner = ?4,
                    draw = ?5,
                    error = ?6,
                    total_turns = ?7,
                    invalid_moves = ?8,
                    total_duration_ms = ?9,
                    result_json = ?10
                 WHERE id = ?1",
                params![
                    run_id,
                    status,
                    finished_at,
                    stats.winner,
                    stats.draw,
                    error,
                    stats.total_turns() as i64,
                    stats.invalid_moves as i64,
                    stats.total_duration_ms as i64,
                    result_json
                ],
            )
            .map_err(|err| err.to_string())?;
            Ok(())
        })
        .await
    }

    /// Append a single turn for callers that persist an event stream as it is emitted.
    pub async fn append_turn(&self, run_id: &str, turn: &TurnStats) -> Result<(), String> {
        let run_id = run_id.to_string();
        let turn = turn.clone();
        self.with_connection(move |conn| {
            ensure_schema(&conn)?;
            let sequence: i64 = conn
                .query_row(
                    "SELECT COALESCE(MAX(seq), -1) + 1 FROM run_turns WHERE run_id = ?1",
                    params![run_id],
                    |row| row.get(0),
                )
                .map_err(|err| err.to_string())?;
            conn.execute(
                "INSERT INTO run_turns (
                    run_id, seq, turn_number, player, move_json, valid, error,
                    time_taken_ms, state_before_json, state_after_json, diagnostics
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    run_id,
                    sequence,
                    turn.turn_number as i64,
                    turn.player,
                    turn.move_made.to_string(),
                    turn.move_valid,
                    turn.error_message,
                    turn.time_taken_ms as i64,
                    turn.state_before.to_string(),
                    turn.state_after.to_string(),
                    turn.diagnostics,
                ],
            )
            .map_err(|err| err.to_string())?;
            Ok(())
        })
        .await
    }

    pub async fn mark_error(&self, run_id: &str, error: &str) -> Result<(), String> {
        let run_id = run_id.to_string();
        let error = error.to_string();
        let finished_at = now_timestamp();
        self.with_connection(move |conn| {
            ensure_schema(&conn)?;
            conn.execute(
                "UPDATE runs SET status = 'error', finished_at = ?2, error = ?3 WHERE id = ?1",
                params![run_id, finished_at, error],
            )
            .map_err(|err| err.to_string())?;
            Ok(())
        })
        .await
    }

    pub async fn list_runs(&self) -> Result<Vec<StoredRun>, String> {
        self.with_connection(|conn| {
            ensure_schema(&conn)?;
            let mut statement = conn
                .prepare(
                    "SELECT id, game_type, status, agents_json, result_json, matrix_job_id
                     FROM runs
                     WHERE status IN ('completed', 'error')
                     ORDER BY created_at DESC, id DESC",
                )
                .map_err(|err| err.to_string())?;
            let rows = statement
                .query_map([], stored_run_from_row)
                .map_err(|err| err.to_string())?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|err| err.to_string())
        })
        .await
    }

    pub async fn get_run(&self, run_id: &str) -> Result<Option<StoredRun>, String> {
        let run_id = run_id.to_string();
        self.with_connection(move |conn| {
            ensure_schema(&conn)?;
            let mut statement = conn
                .prepare(
                    "SELECT id, game_type, status, agents_json, result_json, matrix_job_id
                     FROM runs
                     WHERE id = ?1",
                )
                .map_err(|err| err.to_string())?;
            let mut rows = statement
                .query_map(params![run_id], stored_run_from_row)
                .map_err(|err| err.to_string())?;
            rows.next().transpose().map_err(|err| err.to_string())
        })
        .await
    }

    pub async fn list_turns(&self, run_id: &str) -> Result<Vec<StoredTurn>, String> {
        let run_id = run_id.to_string();
        self.with_connection(move |conn| {
            ensure_schema(&conn)?;
            let mut statement = conn
                .prepare(
                    "SELECT seq, turn_number, player, move_json, valid, error,
                            time_taken_ms, state_before_json, state_after_json, diagnostics
                     FROM run_turns WHERE run_id = ?1 ORDER BY seq ASC",
                )
                .map_err(|err| err.to_string())?;
            let rows = statement
                .query_map(params![run_id], stored_turn_from_row)
                .map_err(|err| err.to_string())?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|err| err.to_string())
        })
        .await
    }

    /// Aggregate model outcomes from durable run history. Prompt file contents
    /// and API credentials are never read from, or included in, this output.
    pub async fn leaderboard(&self) -> Result<Vec<LeaderboardEntry>, String> {
        let mut entries: HashMap<String, LeaderboardAccumulator> = HashMap::new();
        for run in self.list_runs().await? {
            let Some(result) = run.result else {
                continue;
            };
            let stats = result.stats();
            let winner = stats.winner.as_deref().and_then(winner_agent_index);
            for (index, agent) in run.agents.iter().enumerate() {
                let entry = entries.entry(agent.model.clone()).or_default();
                entry.games += 1;
                if stats.draw {
                    entry.draws += 1;
                } else if winner == Some(index) {
                    entry.wins += 1;
                } else if winner.is_some() {
                    entry.losses += 1;
                }
                for turn in &stats.turns {
                    if turn.player.contains(&format!("_{}", index + 1)) {
                        entry.turns += 1;
                        entry.total_turn_time_ms += turn.time_taken_ms;
                        if !turn.move_valid {
                            entry.invalid_moves += 1;
                        }
                    }
                }
            }
        }
        let mut leaderboard = entries
            .into_iter()
            .map(|(model, entry)| LeaderboardEntry {
                model,
                games: entry.games,
                wins: entry.wins,
                losses: entry.losses,
                draws: entry.draws,
                invalid_moves: entry.invalid_moves,
                average_turn_time_ms: if entry.turns == 0 {
                    0.0
                } else {
                    entry.total_turn_time_ms as f64 / entry.turns as f64
                },
            })
            .collect::<Vec<_>>();
        leaderboard.sort_by(|left, right| {
            right
                .wins
                .cmp(&left.wins)
                .then_with(|| right.games.cmp(&left.games))
                .then_with(|| left.model.cmp(&right.model))
        });
        Ok(leaderboard)
    }

    async fn with_connection<T, F>(&self, task: F) -> Result<T, String>
    where
        T: Send + 'static,
        F: FnOnce(Connection) -> Result<T, String> + Send + 'static,
    {
        let _guard = self.lock.lock().await;
        let path = self.path.as_ref().clone();
        tokio::task::spawn_blocking(move || {
            if let Some(parent) = path
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
            {
                std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
            }
            let conn = Connection::open(Path::new(&path)).map_err(|err| err.to_string())?;
            task(conn)
        })
        .await
        .map_err(|err| err.to_string())?
    }
}

#[async_trait::async_trait]
impl GameStorage for RunStore {
    async fn create_run(
        &self,
        game: &Game,
        agents: &[AIAgentConfig],
        metadata: Option<RunMetadata>,
    ) -> Result<String, String> {
        RunStore::create_run(self, game, agents, metadata).await
    }

    async fn append_event(&self, run_id: &str, turn: &TurnStats) -> Result<(), String> {
        self.append_turn(run_id, turn).await
    }

    async fn finish_run(&self, run_id: &str, result: &TestResult) -> Result<(), String> {
        self.complete_run(run_id, result).await
    }

    async fn get_run(&self, run_id: &str) -> Result<Option<StoredRun>, String> {
        RunStore::get_run(self, run_id).await
    }

    async fn list_events(&self, run_id: &str) -> Result<Vec<StoredTurn>, String> {
        self.list_turns(run_id).await
    }
}

fn ensure_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS runs (
            id VARCHAR PRIMARY KEY,
            game_type VARCHAR NOT NULL,
            status VARCHAR NOT NULL,
            created_at VARCHAR NOT NULL,
            started_at VARCHAR,
            finished_at VARCHAR,
            winner VARCHAR,
            draw BOOLEAN NOT NULL DEFAULT false,
            error VARCHAR,
            total_turns BIGINT,
            invalid_moves BIGINT,
            total_duration_ms BIGINT,
            agents_json VARCHAR NOT NULL,
            game_config_json VARCHAR NOT NULL,
            result_json VARCHAR,
            matrix_job_id VARCHAR,
            metadata_json VARCHAR
        );

        CREATE TABLE IF NOT EXISTS run_turns (
            run_id VARCHAR NOT NULL,
            seq BIGINT NOT NULL,
            turn_number BIGINT NOT NULL,
            player VARCHAR NOT NULL,
            move_json VARCHAR NOT NULL,
            valid BOOLEAN NOT NULL,
            error VARCHAR,
            time_taken_ms BIGINT NOT NULL,
            state_before_json VARCHAR NOT NULL,
            state_after_json VARCHAR NOT NULL,
            diagnostics VARCHAR,
            PRIMARY KEY (run_id, seq)
        );

        CREATE INDEX IF NOT EXISTS idx_runs_status_created ON runs(status, created_at);
        ALTER TABLE runs ADD COLUMN IF NOT EXISTS matrix_job_id VARCHAR;
        CREATE INDEX IF NOT EXISTS idx_runs_matrix ON runs(matrix_job_id, created_at);
        CREATE INDEX IF NOT EXISTS idx_run_turns_run ON run_turns(run_id, seq);

        CREATE TABLE IF NOT EXISTS schema_migrations (
            version BIGINT PRIMARY KEY,
            applied_at VARCHAR NOT NULL
        );

        INSERT INTO schema_migrations (version, applied_at)
        VALUES (1, '2026-07-16T00:00:00.000Z')
        ON CONFLICT (version) DO NOTHING;
        ",
    )
    .map_err(|err| err.to_string())
}

fn stored_run_from_row(row: &duckdb::Row<'_>) -> duckdb::Result<StoredRun> {
    let agents_json: String = row.get(3)?;
    let result_json: Option<String> = row.get(4)?;
    Ok(StoredRun {
        id: row.get(0)?,
        game_type: row.get(1)?,
        status: row.get(2)?,
        agents: serde_json::from_str(&agents_json).unwrap_or_default(),
        result: result_json.and_then(|json| serde_json::from_str(&json).ok()),
        matrix_job_id: row.get(5)?,
    })
}

fn stored_turn_from_row(row: &duckdb::Row<'_>) -> duckdb::Result<StoredTurn> {
    let move_json: String = row.get(3)?;
    let state_before_json: String = row.get(7)?;
    let state_after_json: String = row.get(8)?;
    Ok(StoredTurn {
        sequence: row.get::<_, i64>(0)? as u64,
        turn: TurnStats {
            turn_number: row.get::<_, i64>(1)? as u32,
            player: row.get(2)?,
            move_made: serde_json::from_str(&move_json).unwrap_or(serde_json::Value::Null),
            move_valid: row.get(4)?,
            error_message: row.get(5)?,
            time_taken_ms: row.get::<_, i64>(6)? as u64,
            state_before: serde_json::from_str(&state_before_json)
                .unwrap_or(serde_json::Value::Null),
            state_after: serde_json::from_str(&state_after_json).unwrap_or(serde_json::Value::Null),
            diagnostics: row.get(9)?,
        },
    })
}

fn winner_agent_index(winner: &str) -> Option<usize> {
    let (_, suffix) = winner.split_once('_')?;
    let digits = suffix
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .collect::<String>();
    digits.parse::<usize>().ok()?.checked_sub(1)
}

fn to_json_string<T: Serialize + ?Sized>(value: &T) -> Result<String, String> {
    serde_json::to_string(value).map_err(|err| err.to_string())
}

fn now_timestamp() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}.{:03}Z", now.as_secs(), now.subsec_millis())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_config::AgentKind;
    use crate::games::game::{TicTacToeConfig, TicTacToeResult};
    use crate::games::stats::{GameStats, TurnStats};
    use serde_json::json;

    fn test_store() -> RunStore {
        RunStore::new(
            std::env::temp_dir().join(format!("ai_arena_storage_test_{}.duckdb", Uuid::new_v4())),
        )
    }

    fn test_agents() -> Vec<AIAgentConfig> {
        vec![
            AIAgentConfig {
                model: "model-a".to_string(),
                temp: 0.7,
                seed: Some(1),
                agent: AgentKind::OpenAI,
                secret_profile: None,
                api_endpoint: None,
                prompt_file: None,
            },
            AIAgentConfig {
                model: "model-b".to_string(),
                temp: 0.2,
                seed: Some(2),
                agent: AgentKind::Ollama,
                secret_profile: None,
                api_endpoint: Some("http://localhost:11434".to_string()),
                prompt_file: None,
            },
        ]
    }

    fn test_result() -> TestResult {
        let mut stats = GameStats::new();
        stats.winner = Some("OpenAI_1 (X)".to_string());
        stats.total_duration_ms = 42;
        stats.add_turn(TurnStats {
            turn_number: 1,
            player: "OpenAI_1".to_string(),
            move_made: json!({"row": 0, "col": 0}),
            time_taken_ms: 7,
            move_valid: true,
            error_message: None,
            state_before: json!({"board": []}),
            state_after: json!({"board": [["X"]]}),
            diagnostics: Some("ok".to_string()),
        });

        TestResult::TicTacToe(TicTacToeResult {
            winner: stats.winner.clone(),
            stats,
            error: None,
        })
    }

    #[tokio::test]
    async fn stores_completed_run_and_turns() {
        let store = test_store();
        store.initialize().await.unwrap();
        let game = Game::TicTacToe(TicTacToeConfig::default());
        let agents = test_agents();

        let mut metadata = RunMetadata::source("test");
        metadata.matrix_job_id = Some("matrix-test".to_string());
        let run_id = store
            .create_run(&game, &agents, Some(metadata))
            .await
            .unwrap();
        store.mark_running(&run_id).await.unwrap();
        store.complete_run(&run_id, &test_result()).await.unwrap();

        let runs = store.list_runs().await.unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].id, run_id);
        assert_eq!(runs[0].status, "completed");
        assert_eq!(runs[0].agents.len(), 2);
        assert!(runs[0].result.is_some());
        assert_eq!(runs[0].matrix_job_id.as_deref(), Some("matrix-test"));

        let turns = store.list_turns(&run_id).await.unwrap();
        assert_eq!(turns.len(), 1);
        assert_eq!(turns[0].sequence, 0);
        assert_eq!(turns[0].turn.move_made, json!({"row": 0, "col": 0}));

        let storage: &dyn GameStorage = &store;
        assert_eq!(storage.list_events(&run_id).await.unwrap().len(), 1);

        let leaderboard = store.leaderboard().await.unwrap();
        assert_eq!(leaderboard[0].model, "model-a");
        assert_eq!(leaderboard[0].wins, 1);
        assert_eq!(leaderboard[1].model, "model-b");
        assert_eq!(leaderboard[1].losses, 1);
    }

    #[tokio::test]
    async fn stores_error_run() {
        let store = test_store();
        let game = Game::TicTacToe(TicTacToeConfig::default());
        let agents = test_agents();

        let run_id = store.create_run(&game, &agents, None).await.unwrap();
        store.mark_running(&run_id).await.unwrap();
        store.mark_error(&run_id, "boom").await.unwrap();

        let run = store.get_run(&run_id).await.unwrap().unwrap();
        assert_eq!(run.status, "error");
        assert!(run.result.is_none());
    }
}
