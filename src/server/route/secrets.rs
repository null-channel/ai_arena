use crate::server::state::AppState;
use axum::{Json, Router, extract::State, routing::get};
use serde::Serialize;

pub(super) fn routes() -> Router<AppState> {
    Router::new().route("/api/secrets", get(get_secrets))
}

#[derive(Debug, Serialize)]
pub struct SecretsResponse {
    pub profiles: Vec<String>,
}

async fn get_secrets(State(state): State<AppState>) -> Json<SecretsResponse> {
    let secrets = state.secrets.lock().await;
    let profiles = secrets.list_profiles();
    println!("API: Returning secret profiles: {:?}", profiles);
    Json(SecretsResponse { profiles })
}
