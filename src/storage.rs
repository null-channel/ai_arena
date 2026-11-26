use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
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
    backend: Arc<Mutex<Database>>,
}

#[derive(Debug)]
enum Database {
    File(PathBuf),
    Memory(Option<Connection>),
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

pub struct BatchSummary {
    pub id: String,
    pub game_type: String,
    pub total: i64,
    pub finished: i64,
}

pub struct BatchResult {
    pub id: String,
    pub total: i64,
    pub draws: i64,
    pub failed: i64,
    pub finished_at: String,
}

pub struct BatchSession {
    pub id: String,
    pub game_type: String,
    pub agents: Vec<AIAgentConfig>,
    pub status: String,
    pub repetition: Option<u32>,
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
    pub opponents: BTreeMap<String, MatchRecord>,
}

pub const HISTORY_PAGE_SIZE: u32 = 25;

#[derive(Default)]
pub struct GameHistory {
    pub completed: i64,
    pub failed: i64,
    pub draws: i64,
    pub avg_duration_ms: Option<f64>,
    pub runs: Vec<RunMetrics>,
}

pub struct RunMetrics {
    pub id: String,
    pub listed_at: String,
    pub agents: Vec<AIAgentConfig>,
    pub outcome: String,
    pub turns: Option<i64>,
    pub invalid_moves: Option<i64>,
    pub duration_ms: Option<i64>,
}

#[derive(Clone, Debug, Default, Serialize, PartialEq, Eq)]
pub struct MatchRecord {
    pub games: u32,
    pub wins: u32,
    pub losses: u32,
    pub draws: u32,
}

impl MatchRecord {
    fn add_appearance(&mut self, player: usize, winner: Option<usize>, draw: bool) {
        self.games += 1;
        if draw {
            self.draws += 1;
        } else if let Some(winner) = winner {
            if player == winner {
                self.wins += 1;
            } else {
                self.losses += 1;
            }
        }
    }
}

#[derive(Default)]
struct LeaderboardAccumulator {
    record: MatchRecord,
    opponents: BTreeMap<String, MatchRecord>,
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

#[derive(Clone, Debug, Serialize, Deserialize)]
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
            backend: Arc::new(Mutex::new(Database::File(path.into()))),
        }
    }

    pub fn in_memory() -> Self {
        Self {
            backend: Arc::new(Mutex::new(Database::Memory(None))),
        }
    }

    pub fn default_path() -> PathBuf {
        PathBuf::from(DEFAULT_DB_PATH)
    }

    pub async fn initialize(&self) -> Result<(), String> {
        self.with_connection(|conn| {
            ensure_schema(conn)?;
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
        let ids = self
            .create_runs(game, vec![(agents.to_vec(), metadata)])
            .await?;
        Ok(ids.into_iter().next().expect("one inserted run"))
    }

    pub async fn create_runs(
        &self,
        game: &Game,
        runs: Vec<(Vec<AIAgentConfig>, Option<RunMetadata>)>,
    ) -> Result<Vec<String>, String> {
        let game_type = game.name().to_owned();
        let game_config_json = to_json_string(game)?;
        self.with_connection(move |conn| {
            ensure_schema(conn)?;
            let tx = conn.transaction().map_err(|err| err.to_string())?;
            let mut ids = Vec::with_capacity(runs.len());
            {
                let mut insert = tx.prepare("INSERT INTO runs (
                    id, game_type, status, created_at, agents_json, game_config_json, matrix_job_id, metadata_json, batch_mode
                ) VALUES (?1, ?2, 'pending', ?3, ?4, ?5, ?6, ?7, ?8)").map_err(|err| err.to_string())?;
                for (agents, metadata) in runs {
                    let id = Uuid::new_v4().to_string();
                    let job_id = metadata.as_ref().and_then(|meta| meta.matrix_job_id.as_deref());
                    insert.execute(params![id, game_type, now_timestamp(), to_json_string(&agents)?,
                        game_config_json, job_id, metadata.as_ref().map(to_json_string).transpose()?,
                        metadata.as_ref().filter(|_| job_id.is_some()).and_then(|m| m.source.as_deref())])
                        .map_err(|err| err.to_string())?;
                    ids.push(id);
                }
            }
            tx.commit().map_err(|err| err.to_string())?;
            Ok(ids)
        }).await
    }

    pub async fn mark_running(&self, run_id: &str) -> Result<(), String> {
        let run_id = run_id.to_string();
        let started_at = now_timestamp();
        self.with_connection(move |conn| {
            ensure_schema(conn)?;
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
            ensure_schema(conn)?;
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
            ensure_schema(conn)?;
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
            ensure_schema(conn)?;
            conn.execute(
                "UPDATE runs SET status = 'error', finished_at = ?2, error = ?3 WHERE id = ?1",
                params![run_id, finished_at, error],
            )
            .map_err(|err| err.to_string())?;
            Ok(())
        })
        .await
    }

    pub async fn list_runs(&self, game: Option<&Game>) -> Result<Vec<StoredRun>, String> {
        let game_type = game.map(|g| g.name().to_owned());
        self.with_connection(move |conn| {
            ensure_schema(conn)?;
            let mut statement = conn
                .prepare(
                    "SELECT id, game_type, status, agents_json, result_json, matrix_job_id
                     FROM runs
                     WHERE status IN ('completed', 'error') AND (?1 IS NULL OR game_type = ?1)
                     ORDER BY created_at DESC, id DESC",
                )
                .map_err(|err| err.to_string())?;
            let rows = statement
                .query_map(params![game_type], stored_run_from_row)
                .map_err(|err| err.to_string())?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|err| err.to_string())
        })
        .await
    }

    pub async fn list_batches(&self, mode: &str) -> Result<Vec<BatchSummary>, String> {
        let mode = mode.to_owned();
        self.with_connection(move |conn| {
            ensure_schema(conn)?;
            // Older batches were all recorded as matrix jobs.
            let mut query = conn.prepare(
                "SELECT matrix_job_id, game_type, count(*), count(*) FILTER (WHERE status IN ('completed', 'error'))
                 FROM runs WHERE matrix_job_id IS NOT NULL AND coalesce(batch_mode, 'matrix') = ?1
                 GROUP BY matrix_job_id, game_type ORDER BY min(created_at) DESC, matrix_job_id"
            ).map_err(|err| err.to_string())?;
            let rows = query.query_map(params![mode], |row| Ok(BatchSummary {
                id: row.get(0)?, game_type: row.get(1)?, total: row.get(2)?, finished: row.get(3)?,
            })).map_err(|err| err.to_string())?;
            rows.collect::<Result<Vec<_>, _>>().map_err(|err| err.to_string())
        }).await
    }

    pub async fn recent_batch_results(
        &self,
        mode: &str,
        game: &Game,
    ) -> Result<Vec<BatchResult>, String> {
        let mode = mode.to_owned();
        let game_type = game.name().to_owned();
        self.with_connection(move |conn| {
            ensure_schema(conn)?;
            let mut query = conn.prepare(
                "WITH batches AS (
                    SELECT matrix_job_id, count(*) AS total,
                           count(*) FILTER (WHERE status = 'completed' AND draw) AS draws,
                           count(*) FILTER (WHERE status = 'error') AS failed,
                           max(coalesce(finished_at, created_at)) AS last_finish
                    FROM runs WHERE game_type = ?1 AND matrix_job_id IS NOT NULL
                          AND coalesce(batch_mode, 'matrix') = ?2
                    GROUP BY matrix_job_id
                    HAVING count(*) = count(*) FILTER (WHERE status IN ('completed', 'error'))
                 )
                 SELECT matrix_job_id, total, draws, failed,
                        coalesce(strftime(epoch_ms(try_cast(try_cast(rtrim(last_finish, 'Z') AS DOUBLE) * 1000 AS BIGINT)),
                                         '%Y-%m-%d %H:%M'), last_finish)
                 FROM batches ORDER BY last_finish DESC, matrix_job_id DESC LIMIT 3"
            ).map_err(|err| err.to_string())?;
            let rows = query.query_map(params![game_type, mode], |row| Ok(BatchResult {
                id: row.get(0)?, total: row.get(1)?, draws: row.get(2)?, failed: row.get(3)?, finished_at: row.get(4)?,
            })).map_err(|err| err.to_string())?;
            rows.collect::<Result<Vec<_>, _>>().map_err(|err| err.to_string())
        }).await
    }

    pub async fn batch_sessions(&self, mode: &str, id: &str) -> Result<Vec<BatchSession>, String> {
        let (mode, id) = (mode.to_owned(), id.to_owned());
        self.with_connection(move |conn| {
            ensure_schema(conn)?;
            let mut query = conn
                .prepare(
                    "SELECT id, game_type, status, agents_json, metadata_json FROM runs
                 WHERE matrix_job_id = ?1 AND coalesce(batch_mode, 'matrix') = ?2
                 ORDER BY created_at, rowid",
                )
                .map_err(|err| err.to_string())?;
            let rows = query
                .query_map(params![id, mode], |row| {
                    let agents: String = row.get(3)?;
                    let metadata: Option<String> = row.get(4)?;
                    Ok(BatchSession {
                        id: row.get(0)?,
                        game_type: row.get(1)?,
                        status: row.get(2)?,
                        agents: serde_json::from_str(&agents).unwrap_or_default(),
                        repetition: metadata
                            .and_then(|v| serde_json::from_str::<RunMetadata>(&v).ok())
                            .and_then(|m| m.repetition),
                    })
                })
                .map_err(|err| err.to_string())?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|err| err.to_string())
        })
        .await
    }

    pub async fn parent_batch(&self, run_id: &str) -> Result<Option<(String, String)>, String> {
        let id = run_id.to_owned();
        self.with_connection(move |conn| {
            ensure_schema(conn)?;
            let mut query = conn.prepare(
                "SELECT coalesce(batch_mode, 'matrix'), matrix_job_id FROM runs WHERE id = ?1 AND matrix_job_id IS NOT NULL"
            ).map_err(|err| err.to_string())?;
            let mut rows = query.query_map(params![id], |row| Ok((row.get(0)?, row.get(1)?)))
                .map_err(|err| err.to_string())?;
            rows.next().transpose().map_err(|err| err.to_string())
        }).await
    }

    pub async fn recent_single_results(&self, game: &Game) -> Result<Vec<RunMetrics>, String> {
        let game_type = game.name().to_owned();
        self.with_connection(move |conn| {
            ensure_schema(conn)?;
            finished_run_metrics(conn, &game_type, 3, 0, true)
        })
        .await
    }

    /// Page over scalar metrics without loading move logs or board snapshots.
    pub async fn game_history(&self, game: &Game, page: u32) -> Result<GameHistory, String> {
        let game_type = game.name().to_owned();
        self.with_connection(move |conn| {
            ensure_schema(conn)?;
            let mut history = conn
                .query_row(
                    "SELECT count(*) FILTER (WHERE status = 'completed'),
                        count(*) FILTER (WHERE status = 'error'),
                        count(*) FILTER (WHERE status = 'completed' AND draw),
                        avg(total_duration_ms) FILTER (WHERE status = 'completed')
                 FROM runs WHERE game_type = ?1",
                    params![game_type],
                    |row| {
                        Ok(GameHistory {
                            completed: row.get(0)?,
                            failed: row.get(1)?,
                            draws: row.get(2)?,
                            avg_duration_ms: row.get(3)?,
                            runs: Vec::new(),
                        })
                    },
                )
                .map_err(|err| err.to_string())?;
            let offset = i64::from(page.saturating_sub(1)) * i64::from(HISTORY_PAGE_SIZE);
            history.runs =
                finished_run_metrics(conn, &game_type, HISTORY_PAGE_SIZE, offset, false)?;
            Ok(history)
        })
        .await
    }

    pub async fn get_run(&self, run_id: &str) -> Result<Option<StoredRun>, String> {
        let run_id = run_id.to_string();
        self.with_connection(move |conn| {
            ensure_schema(conn)?;
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
            ensure_schema(conn)?;
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
    pub async fn leaderboard(&self, game: Option<&Game>) -> Result<Vec<LeaderboardEntry>, String> {
        let mut entries: HashMap<String, LeaderboardAccumulator> = HashMap::new();
        for run in self.list_runs(game).await? {
            let Some(result) = run.result else {
                continue;
            };
            let stats = result.stats();
            let winner = stats.winner.as_deref().and_then(winner_agent_index);
            for (index, agent) in run.agents.iter().enumerate() {
                let entry = entries.entry(agent.model.clone()).or_default();
                entry.record.add_appearance(index, winner, stats.draw);
                if let [first, second] = run.agents.as_slice() {
                    let opponent = if index == 0 { second } else { first };
                    entry
                        .opponents
                        .entry(opponent.model.clone())
                        .or_default()
                        .add_appearance(index, winner, stats.draw);
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
                games: entry.record.games,
                wins: entry.record.wins,
                losses: entry.record.losses,
                draws: entry.record.draws,
                opponents: entry.opponents,
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
        F: FnOnce(&mut Connection) -> Result<T, String> + Send + 'static,
    {
        let mut backend = self.backend.clone().lock_owned().await;
        tokio::task::spawn_blocking(move || match &mut *backend {
            Database::File(path) => {
                if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
                    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                }
                let mut conn = Connection::open(path).map_err(|e| e.to_string())?;
                task(&mut conn)
            }
            Database::Memory(slot) => {
                let conn = match slot {
                    Some(conn) => conn,
                    None => slot.insert(Connection::open_in_memory().map_err(|e| e.to_string())?),
                };
                task(conn)
            }
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
        ALTER TABLE runs ADD COLUMN IF NOT EXISTS batch_mode VARCHAR;
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

fn finished_run_metrics(
    conn: &Connection,
    game: &str,
    limit: u32,
    offset: i64,
    single_only: bool,
) -> Result<Vec<RunMetrics>, String> {
    let mut query = conn.prepare(
        "WITH selected AS (
             SELECT *, CASE WHEN ?4 THEN coalesce(finished_at, created_at) ELSE created_at END AS listed_at
             FROM runs WHERE game_type = ?1 AND status IN ('completed', 'error')
                       AND (NOT ?4 OR matrix_job_id IS NULL)
         )
         SELECT id, coalesce(strftime(epoch_ms(try_cast(try_cast(rtrim(listed_at, 'Z') AS DOUBLE) * 1000 AS BIGINT)),
                                      '%Y-%m-%d %H:%M'), listed_at),
                agents_json, status, winner, draw, total_turns, invalid_moves, total_duration_ms
         FROM selected ORDER BY listed_at DESC, id DESC LIMIT ?2 OFFSET ?3"
    ).map_err(|err| err.to_string())?;
    let rows = query
        .query_map(
            params![game, limit, offset, single_only],
            run_metrics_from_row,
        )
        .map_err(|err| err.to_string())?;
    rows.collect::<Result<_, _>>()
        .map_err(|err| err.to_string())
}

fn run_metrics_from_row(row: &duckdb::Row<'_>) -> duckdb::Result<RunMetrics> {
    let agents: Vec<AIAgentConfig> =
        serde_json::from_str(&row.get::<_, String>(2)?).unwrap_or_default();
    let winner: Option<String> = row.get(4)?;
    let outcome = if row.get::<_, String>(3)? == "error" {
        "Failed".into()
    } else if row.get::<_, bool>(5)? {
        "Draw".into()
    } else if let Some(winner) = winner {
        let label = winner_agent_index(&winner)
            .and_then(|index| agents.get(index))
            .map_or(winner.as_str(), |agent| agent.model.as_str());
        format!("{label} won")
    } else {
        "Completed".into()
    };
    Ok(RunMetrics {
        id: row.get(0)?,
        listed_at: row.get(1)?,
        agents,
        outcome,
        turns: row.get(6)?,
        invalid_moves: row.get(7)?,
        duration_ms: row.get(8)?,
    })
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

        let runs = store.list_runs(None).await.unwrap();
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

        let leaderboard = store.leaderboard(Some(&game)).await.unwrap();
        assert_eq!(leaderboard[0].model, "model-a");
        assert_eq!(leaderboard[0].wins, 1);
        assert_eq!(leaderboard[1].model, "model-b");
        assert_eq!(leaderboard[1].losses, 1);
    }

    #[tokio::test]
    async fn leaderboard_counts_only_appearances_in_the_selected_game() {
        let dir = tempfile::tempdir().unwrap();
        let store = RunStore::new(dir.path().join("rankings.duckdb"));
        let mut agents = test_agents();
        let tictactoe = Game::from("TicTacToe");
        let connect_four = Game::from("ConnectFour");
        let first = store.create_run(&tictactoe, &agents, None).await.unwrap();
        store.complete_run(&first, &test_result()).await.unwrap();

        agents[1].model = "model-c".into();
        let mut stats = GameStats::new();
        stats.draw = true;
        let draw = TestResult::ConnectFour(crate::games::game::ConnectFourResult {
            winner: None,
            stats,
            error: None,
        });
        let second = store
            .create_run(&connect_four, &agents, None)
            .await
            .unwrap();
        store.complete_run(&second, &draw).await.unwrap();

        let board = store.leaderboard(Some(&connect_four)).await.unwrap();
        assert_eq!(board.len(), 2);
        assert_eq!(
            (
                board[0].model.as_str(),
                board[0].games,
                board[0].wins,
                board[0].draws
            ),
            ("model-a", 1, 0, 1)
        );
        assert_eq!(board[1].model, "model-c");
        assert_eq!(board[0].opponents.len(), 1);
        assert_eq!(board[0].opponents["model-c"].draws, 1);
        let combined = store.leaderboard(None).await.unwrap();
        assert_eq!(
            (combined[0].games, combined[0].wins, combined[0].draws),
            (2, 1, 1)
        );
    }

    #[tokio::test]
    async fn head_to_head_records_follow_seats_and_reconcile_with_the_leaderboard() {
        let dir = tempfile::tempdir().unwrap();
        let store = RunStore::new(dir.path().join("opponents.duckdb"));
        let game = Game::from("TicTacToe");
        let mut agents = test_agents();
        for (first, second) in [("a", "b"), ("b", "a"), ("a", "c"), ("a", "a")] {
            agents[0].model = first.into();
            agents[1].model = second.into();
            let id = store.create_run(&game, &agents, None).await.unwrap();
            store.complete_run(&id, &test_result()).await.unwrap();
        }
        let board = store.leaderboard(Some(&game)).await.unwrap();
        let a = &board[0];
        assert_eq!(a.model, "a");
        assert_eq!((a.games, a.wins, a.losses), (5, 3, 2));
        for (model, games, wins, losses) in [("a", 2, 1, 1), ("b", 2, 1, 1), ("c", 1, 1, 0)] {
            let record = &a.opponents[model];
            assert_eq!(
                (record.games, record.wins, record.losses),
                (games, wins, losses)
            );
        }
    }

    #[tokio::test]
    async fn history_filters_finished_runs_and_keeps_missing_metrics_distinct_from_zero() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.duckdb");
        let store = RunStore::new(&path);
        let game = Game::from("TicTacToe");
        let agents = test_agents();
        let finished = store.create_run(&game, &agents, None).await.unwrap();
        store.complete_run(&finished, &test_result()).await.unwrap();
        let failed = store.create_run(&game, &agents, None).await.unwrap();
        store.mark_error(&failed, "offline").await.unwrap();
        store.create_run(&game, &agents, None).await.unwrap();
        let chess = store
            .create_run(&Game::from("Chess"), &agents, None)
            .await
            .unwrap();
        store.mark_error(&chess, "offline").await.unwrap();

        let history = RunStore::new(path).game_history(&game, 1).await.unwrap();
        assert_eq!(
            (history.completed, history.failed, history.draws),
            (1, 1, 0)
        );
        assert_eq!(history.avg_duration_ms, Some(42.0));
        assert_eq!(history.runs.len(), 2);
        assert_eq!(history.runs[0].id, failed);
        assert_eq!(history.runs[0].turns, None);
        assert_eq!(history.runs[1].id, finished);
        assert_eq!(history.runs[1].turns, Some(1));
        assert_eq!(history.runs[1].invalid_moves, Some(0));
        assert_eq!(history.runs[1].outcome, "model-a won");
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
