use crate::server::{error::AppError, htmx, state::AppState};
use axum::{Json, Router, extract::State, response::Html, routing::get};

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/leaderboard", get(leaderboard_fragment))
        .route("/api/leaderboard", get(get_leaderboard))
}

async fn leaderboard_fragment(State(state): State<AppState>) -> Html<String> {
    Html(htmx::render_leaderboard_fragment(
        state.store.leaderboard().await,
    ))
}

async fn get_leaderboard(
    State(state): State<AppState>,
) -> Result<Json<Vec<crate::storage::LeaderboardEntry>>, AppError> {
    state
        .store
        .leaderboard()
        .await
        .map(Json)
        .map_err(AppError::Internal)
}
