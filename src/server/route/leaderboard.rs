use super::GameQuery;
use crate::server::{error::AppError, htmx, state::AppState};
use axum::{
    Json, Router,
    extract::{Query, State},
    response::Html,
    routing::get,
};

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/rankings", get(rankings_page))
        .route("/leaderboard", get(leaderboard_fragment))
        .route("/api/leaderboard", get(get_leaderboard))
}

async fn rankings_page(
    State(state): State<AppState>,
    Query(query): Query<GameQuery>,
) -> Result<Html<String>, AppError> {
    let game = query.selected_game()?;
    let scores = htmx::render_leaderboard_fragment(state.store.leaderboard(Some(&game)).await);
    Ok(Html(htmx::render_rankings_page(
        state.mode(),
        game.name(),
        &scores,
    )))
}

async fn leaderboard_fragment(
    State(state): State<AppState>,
    Query(query): Query<GameQuery>,
) -> Result<Html<String>, AppError> {
    let game = query.selected_game()?;
    Ok(Html(htmx::render_leaderboard_fragment(
        state.store.leaderboard(Some(&game)).await,
    )))
}

async fn get_leaderboard(
    State(state): State<AppState>,
    Query(query): Query<GameQuery>,
) -> Result<Json<Vec<crate::storage::LeaderboardEntry>>, AppError> {
    // Existing API clients can still request the combined rankings.
    let game = query
        .game
        .as_ref()
        .map(|_| query.selected_game())
        .transpose()?;
    state
        .store
        .leaderboard(game.as_ref())
        .await
        .map(Json)
        .map_err(AppError::Internal)
}
