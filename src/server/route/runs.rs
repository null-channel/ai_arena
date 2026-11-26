use super::GameQuery;
use crate::games::game::Game;
use crate::server::{config::RunMode, error::AppError, htmx, state::AppState};
use axum::{
    Extension, Json, Router,
    extract::{Path, Query, State},
    response::Html,
    routing::get,
};
use serde::Deserialize;

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/history", get(history_page))
        .route(
            "/history/summary",
            get(recent_results).layer(Extension(RunMode::Single)),
        )
        .route("/games/{id}/replay", get(replay_page))
        .route("/api/runs/{id}", get(get_stored_run))
        .route("/api/runs/{id}/events", get(get_stored_run_events))
}

pub(super) async fn recent_results(
    Extension(mode): Extension<RunMode>,
    State(state): State<AppState>,
    Query(query): Query<GameQuery>,
) -> Result<Html<String>, AppError> {
    let game = query.selected_game()?;
    load_recent_results(&state, &game, mode).await.map(Html)
}

pub(super) async fn load_recent_results(
    state: &AppState,
    game: &Game,
    mode: RunMode,
) -> Result<String, AppError> {
    if mode == RunMode::Single {
        let runs = state
            .store
            .recent_single_results(game)
            .await
            .map_err(AppError::Internal)?;
        Ok(htmx::render_recent_results(game.name(), &runs))
    } else {
        let batches = state
            .store
            .recent_batch_results(mode.path().trim_start_matches('/'), game)
            .await
            .map_err(AppError::Internal)?;
        Ok(htmx::render_recent_batches(game.name(), mode, &batches))
    }
}

async fn history_page(
    State(state): State<AppState>,
    Query(query): Query<GameQuery>,
) -> Result<Html<String>, AppError> {
    let game = query.selected_game()?;
    let page = query.page.unwrap_or(1).max(1);
    let history = state
        .store
        .game_history(&game, page)
        .await
        .map_err(AppError::Internal)?;
    Ok(Html(htmx::render_history(
        state.mode(),
        game.name(),
        page,
        history,
    )))
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
        .game_store(&game_id)
        .await
        .get_run(&game_id)
        .await
        .map_err(AppError::Internal)?
        .ok_or_else(|| AppError::NotFound("Run not found".to_string()))?;
    let events = state
        .game_store(&game_id)
        .await
        .list_turns(&game_id)
        .await
        .map_err(AppError::Internal)?;
    Ok(Html(htmx::render_replay_page(
        state.mode(),
        &game_id,
        &run,
        &events,
        query.turn,
    )))
}

async fn get_stored_run(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> Result<Json<crate::storage::StoredRun>, AppError> {
    let run = state
        .game_store(&run_id)
        .await
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
        .game_store(&run_id)
        .await
        .get_run(&run_id)
        .await
        .map_err(AppError::Internal)?
        .is_none()
    {
        return Err(AppError::NotFound("Run not found".to_string()));
    }
    let events = state
        .game_store(&run_id)
        .await
        .list_turns(&run_id)
        .await
        .map_err(AppError::Internal)?;
    Ok(Json(events))
}
