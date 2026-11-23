use crate::server::{error::AppError, htmx, state::AppState};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    response::Html,
    routing::get,
};
use serde::Deserialize;

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/games/{id}/replay", get(replay_page))
        .route("/api/runs/{id}", get(get_stored_run))
        .route("/api/runs/{id}/events", get(get_stored_run_events))
}

#[derive(Debug, Deserialize, Default)]
struct ReplayQuery {
    turn: Option<usize>,
}

async fn replay_page(
    State(state): State<AppState>,
    Path(game_id): Path<String>,
    Query(query): Query<ReplayQuery>,
) -> Result<Html<String>, AppError> {
    let run = state
        .store
        .get_run(&game_id)
        .await
        .map_err(AppError::Internal)?
        .ok_or_else(|| AppError::NotFound("Run not found".to_string()))?;
    let events = state
        .store
        .list_turns(&game_id)
        .await
        .map_err(AppError::Internal)?;
    Ok(Html(htmx::render_replay_page(
        &game_id, &run, &events, query.turn,
    )))
}

async fn get_stored_run(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> Result<Json<crate::storage::StoredRun>, AppError> {
    let run = state
        .store
        .get_run(&run_id)
        .await
        .map_err(AppError::Internal)?
        .ok_or_else(|| AppError::NotFound("Run not found".to_string()))?;
    Ok(Json(run))
}

async fn get_stored_run_events(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> Result<Json<Vec<crate::storage::StoredTurn>>, AppError> {
    if state
        .store
        .get_run(&run_id)
        .await
        .map_err(AppError::Internal)?
        .is_none()
    {
        return Err(AppError::NotFound("Run not found".to_string()));
    }
    let events = state
        .store
        .list_turns(&run_id)
        .await
        .map_err(AppError::Internal)?;
    Ok(Json(events))
}
