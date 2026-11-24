use super::{
    error::AppError,
    sessions::{GameSession, GameStatus, run_session},
    state::AppState,
};
use crate::agent_config::AIAgentConfig;
use crate::{games::game::Game, storage::RunMetadata};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Semaphore;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub(super) struct CreateMatrixRequest {
    pub game_type: String,
    pub agents: Vec<AIAgentConfig>,
    pub repetitions: u32,
    #[serde(default)]
    pub concurrency_limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum MatrixStatus {
    Queued,
    Running,
    Completed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatrixRun {
    pub models: [String; 2],
    pub repetition: u32,
    pub run_id: String,
    pub status: GameStatus,
}

#[derive(Debug, Clone)]
pub(super) struct MatrixJob {
    id: String,
    game_type: String,
    concurrency_limit: usize,
    status: MatrixStatus,
    runs: Vec<MatrixRun>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MatrixJobResponse {
    pub matrix_id: String,
    pub game_type: String,
    pub status: MatrixStatus,
    pub concurrency_limit: usize,
    pub total: usize,
    pub queued: usize,
    pub running: usize,
    pub completed: usize,
    pub failed: usize,
    pub runs: Vec<MatrixRun>,
}

impl MatrixJob {
    pub(super) fn response(&self) -> MatrixJobResponse {
        let queued = self
            .runs
            .iter()
            .filter(|run| matches!(run.status, GameStatus::Pending))
            .count();
        let running = self
            .runs
            .iter()
            .filter(|run| matches!(run.status, GameStatus::Running))
            .count();
        let completed = self
            .runs
            .iter()
            .filter(|run| matches!(run.status, GameStatus::Completed))
            .count();
        let failed = self
            .runs
            .iter()
            .filter(|run| matches!(run.status, GameStatus::Error))
            .count();
        MatrixJobResponse {
            matrix_id: self.id.clone(),
            game_type: self.game_type.clone(),
            status: self.status.clone(),
            concurrency_limit: self.concurrency_limit,
            total: self.runs.len(),
            queued,
            running,
            completed,
            failed,
            runs: self.runs.clone(),
        }
    }
}

pub(super) const MAX_JOB_RUNS: usize = 1000;

fn pair_schedule(count: usize, repetitions: u32) -> Result<Vec<([usize; 2], u32)>, AppError> {
    if count < 2 || repetitions == 0 {
        return Err(AppError::BadRequest(
            "Choose at least two agents and one match per pair.".into(),
        ));
    }
    let total = count
        .checked_mul(count - 1)
        .and_then(|n| (n / 2).checked_mul(repetitions as usize))
        .filter(|&n| n <= MAX_JOB_RUNS)
        .ok_or_else(|| {
            AppError::BadRequest(format!("A job can contain up to {MAX_JOB_RUNS} matches."))
        })?;
    let mut schedule = Vec::with_capacity(total);
    for repetition in 1..=repetitions {
        for first in 0..count {
            for second in first + 1..count {
                schedule.push(([first, second], repetition));
            }
        }
    }
    Ok(schedule)
}

pub(super) async fn create_matrix_job(
    state: &AppState,
    request: CreateMatrixRequest,
    mode: super::config::RunMode,
) -> Result<MatrixJobResponse, AppError> {
    let game = Game::new(&request.game_type)
        .ok_or_else(|| AppError::BadRequest("Invalid game type".to_string()))?;
    let schedule = pair_schedule(request.agents.len(), request.repetitions)?;
    let matrix_id = Uuid::new_v4().to_string();
    let concurrency_limit = request.concurrency_limit.unwrap_or(4).clamp(1, 32);
    let configurations = schedule
        .iter()
        .map(|&([a, b], repetition)| {
            let agents = vec![request.agents[a].clone(), request.agents[b].clone()];
            let metadata = RunMetadata {
                source: Some(mode.path().trim_start_matches('/').into()),
                description: None,
                repetition: Some(repetition),
                matrix_job_id: Some(matrix_id.clone()),
            };
            (agents, Some(metadata))
        })
        .collect();
    let ids = state
        .store
        .create_runs(&game, configurations)
        .await
        .map_err(AppError::Internal)?;
    let mut runs = Vec::with_capacity(ids.len());
    let mut sessions = state.games.lock().await;
    for (id, ([a, b], repetition)) in ids.into_iter().zip(schedule) {
        let first = &request.agents[a];
        let second = &request.agents[b];
        runs.push(MatrixRun {
            run_id: id.clone(),
            status: GameStatus::Pending,
            models: [first.model.clone(), second.model.clone()],
            repetition,
        });
        sessions.insert(
            id,
            GameSession::pending(game.clone(), vec![first.clone(), second.clone()]),
        );
    }
    drop(sessions);
    let job = MatrixJob {
        id: matrix_id.clone(),
        game_type: request.game_type,
        concurrency_limit,
        status: MatrixStatus::Queued,
        runs,
    };
    let response = job.response();
    state
        .matrix_jobs
        .lock()
        .await
        .insert(matrix_id.clone(), job);
    tokio::spawn(run_matrix_job(state.clone(), matrix_id));
    Ok(response)
}

async fn run_matrix_job(state: AppState, matrix_id: String) {
    let (run_ids, concurrency_limit) = {
        let mut jobs = state.matrix_jobs.lock().await;
        let Some(job) = jobs.get_mut(&matrix_id) else {
            return;
        };
        job.status = MatrixStatus::Running;
        (
            job.runs
                .iter()
                .map(|run| run.run_id.clone())
                .collect::<Vec<_>>(),
            job.concurrency_limit,
        )
    };

    let semaphore = Arc::new(Semaphore::new(concurrency_limit));
    let mut workers = Vec::with_capacity(run_ids.len());
    for run_id in run_ids {
        let state = state.clone();
        let matrix_id = matrix_id.clone();
        let semaphore = semaphore.clone();
        workers.push(tokio::spawn(async move {
            let Ok(_permit) = semaphore.acquire_owned().await else {
                return;
            };
            update_matrix_run_status(&state, &matrix_id, &run_id, GameStatus::Running).await;
            run_session(state.clone(), run_id.clone()).await;
            let status = state
                .games
                .lock()
                .await
                .get(&run_id)
                .map(|session| session.status.clone())
                .unwrap_or(GameStatus::Error);
            update_matrix_run_status(&state, &matrix_id, &run_id, status).await;
        }));
    }
    for worker in workers {
        let _ = worker.await;
    }

    if let Some(job) = state.matrix_jobs.lock().await.get_mut(&matrix_id) {
        job.status = MatrixStatus::Completed;
    }
}

async fn update_matrix_run_status(
    state: &AppState,
    matrix_id: &str,
    run_id: &str,
    status: GameStatus,
) {
    let mut jobs = state.matrix_jobs.lock().await;
    if let Some(job) = jobs.get_mut(matrix_id) {
        if let Some(run) = job.runs.iter_mut().find(|run| run.run_id == run_id) {
            run.status = status;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::{
        route::build_router,
        test::{response_text, test_state},
    };
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;

    #[test]
    fn matrix_schedules_each_unordered_pair_once_per_repetition() {
        let schedule = pair_schedule(3, 3).unwrap();
        assert_eq!(schedule.len(), 9);
        for rep in 1..=3 {
            let pairs: Vec<_> = schedule
                .iter()
                .filter(|(_, r)| *r == rep)
                .map(|(pair, _)| *pair)
                .collect();
            assert_eq!(pairs, [[0, 1], [0, 2], [1, 2]]);
        }
        assert_eq!(pair_schedule(2, 5).unwrap().len(), 5);
        assert!(pair_schedule(1, 3).is_err());
        assert!(pair_schedule(3, 0).is_err());
        assert!(pair_schedule(3, 334).is_err());
    }

    #[tokio::test]
    async fn matrix_status_api_reports_progress() {
        let state = test_state();
        state.matrix_jobs.lock().await.insert(
            "matrix-test".to_string(),
            MatrixJob {
                id: "matrix-test".to_string(),
                game_type: "TicTacToe".to_string(),
                concurrency_limit: 2,
                status: MatrixStatus::Running,
                runs: vec![
                    MatrixRun {
                        models: ["a".into(), "b".into()],
                        repetition: 1,
                        run_id: "run-complete".to_string(),
                        status: GameStatus::Completed,
                    },
                    MatrixRun {
                        models: ["a".into(), "b".into()],
                        repetition: 1,
                        run_id: "run-active".to_string(),
                        status: GameStatus::Running,
                    },
                    MatrixRun {
                        models: ["a".into(), "b".into()],
                        repetition: 1,
                        run_id: "run-error".to_string(),
                        status: GameStatus::Error,
                    },
                ],
            },
        );
        let app = build_router(state, None);

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/matrix/matrix-test")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = response_text(response).await;
        assert!(body.contains("\"completed\":1"));
        assert!(body.contains("\"running\":1"));
        assert!(body.contains("\"failed\":1"));
    }
}
