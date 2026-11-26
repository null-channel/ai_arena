use axum::{
    Extension, Form, Json, Router,
    extract::{Request, State},
    http::StatusCode,
    middleware::{self, Next},
    response::{Html, IntoResponse, Redirect, Response},
    routing::get,
};
use tower_http::{
    cors::{Any, CorsLayer},
    services::ServeDir,
};

use super::{games, leaderboard, matrix, runs, secrets, websocket};
use crate::server::config::RunMode;
use crate::server::{htmx, sessions::live_game_summaries, state::AppState};

/// Build the Axum router (API routes plus optional static file serving).
pub(in crate::server) fn build_router(
    state: AppState,
    static_dir: Option<&std::path::Path>,
) -> Router {
    let api = Router::new()
        .route("/", get(index_page))
        .route("/series", get(async || Redirect::permanent("/multi")))
        .route("/round-robin", get(async || Redirect::permanent("/matrix")))
        .route(
            "/assets/arena.css",
            get(async || {
                (
                    [("content-type", "text/css")],
                    include_str!("../assets/arena.css"),
                )
            }),
        )
        .route(
            "/assets/arena.js",
            get(async || {
                (
                    [("content-type", "text/javascript")],
                    include_str!("../assets/arena.js"),
                )
            }),
        )
        .route(
            "/assets/htmx.js",
            get(async || {
                (
                    [("content-type", "text/javascript")],
                    include_str!("../assets/vendor/htmx-2.0.4.min.js"),
                )
            }),
        )
        .route("/api/health", get(health_check))
        .merge(
            Router::new()
                .route("/api/mode", get(get_mode).post(set_mode))
                .route_layer(middleware::from_fn(super::private_response)),
        )
        .merge(games::routes())
        .merge(super::kubebench::routes())
        .merge(super::toolbench::routes())
        .merge(super::turing::routes())
        .merge(matrix::routes())
        .merge(leaderboard::routes())
        .merge(runs::routes())
        .merge(secrets::routes())
        .merge(websocket::routes())
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .with_state(state.clone());

    let api = if state.auth_token.is_some() {
        api.layer(middleware::from_fn_with_state(state, require_auth))
    } else {
        api
    };

    match static_dir {
        Some(dir) => api.fallback_service(ServeDir::new(dir)),
        None => api,
    }
}

async fn require_auth(State(state): State<AppState>, request: Request, next: Next) -> Response {
    if request.uri().path() == "/api/health" {
        return next.run(request).await;
    }
    let authorized = state.auth_token.as_deref().is_some_and(|token| {
        request
            .headers()
            .get("authorization")
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value == format!("Bearer {token}"))
    });
    if authorized {
        next.run(request).await
    } else {
        (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "Bearer token required"})),
        )
            .into_response()
    }
}

async fn index_page(State(state): State<AppState>) -> Html<String> {
    console_page(state, RunMode::Single).await
}

pub(super) async fn console_page(state: AppState, mode: RunMode) -> Html<String> {
    let profiles = state.secrets.lock().await.list_profiles();
    let defaults = state.form_defaults.lock().await.clone();
    let activity = if mode == RunMode::Single {
        htmx::render_games_fragment(&live_game_summaries(&state).await)
    } else {
        matrix::batch_activity(State(state.clone()), Extension(mode))
            .await
            .map(|Html(activity)| activity)
            .unwrap_or_else(|_| "<p class='empty'>Batch history is unavailable.</p>".into())
    };
    let game = crate::games::game::Game::new(&defaults.game_type);
    let recent = match game {
        Some(game) => runs::load_recent_results(&state, &game, mode)
            .await
            .unwrap_or_else(|_| "<p class='empty'>Run history is unavailable.</p>".into()),
        None => "<p class='empty'>Select a game to see recent results.</p>".into(),
    };
    Html(htmx::render_index_page(
        state.mode(),
        &profiles,
        &defaults,
        &activity,
        mode,
        &recent,
    ))
}

#[derive(serde::Deserialize, serde::Serialize)]
struct ModeSetting {
    mode: crate::server::state::ArenaMode,
}

async fn get_mode(State(state): State<AppState>) -> Json<ModeSetting> {
    Json(ModeSetting { mode: state.mode() })
}

async fn set_mode(
    State(state): State<AppState>,
    Form(setting): Form<ModeSetting>,
) -> Json<ModeSetting> {
    state.set_mode(setting.mode);
    Json(setting)
}

async fn health_check() -> impl IntoResponse {
    Json(serde_json::json!({"status": "ok"}))
}

#[cfg(test)]
mod tests {
    use super::build_router;
    use crate::agent_config::AgentKind;
    use crate::server::{
        config::{AgentFormDefaults, FormDefaults},
        test::{response_text, test_state, test_state_with},
    };
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use tempfile::TempDir;
    use tower::ServiceExt;

    #[tokio::test]
    async fn health_returns_ok() {
        let app = build_router(test_state(), None);
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn optional_bearer_token_protects_server_routes() {
        let state = test_state_with(FormDefaults::default(), Some("arena-token".to_string()));
        let app = build_router(state, None);

        let denied = app
            .clone()
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(denied.status(), StatusCode::UNAUTHORIZED);

        let allowed = app
            .oneshot(
                Request::builder()
                    .uri("/")
                    .header("authorization", "Bearer arena-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(allowed.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn root_page_uses_configured_form_defaults() {
        let state = test_state_with(
            FormDefaults {
                game_type: "ConnectFour".to_string(),
                agent_one: AgentFormDefaults {
                    kind: AgentKind::Gemini,
                    model: "gemini-test".to_string(),
                    temp: 0.2,
                    seed: Some(7),
                    secret_profile: Some("gemini.dev".to_string()),
                    api_endpoint: None,
                    prompt_file: None,
                },
                agent_two: AgentFormDefaults {
                    kind: AgentKind::Anthropic,
                    model: "claude-test".to_string(),
                    temp: 0.4,
                    seed: Some(8),
                    secret_profile: None,
                    api_endpoint: None,
                    prompt_file: None,
                },
            },
            None,
        );
        let app = build_router(state, None);
        let response = app
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = response_text(response).await;
        assert!(body.contains("value=\"ConnectFour\" checked"));
        assert!(body.contains("value=\"gemini-test\""));
        assert!(body.contains("value=\"claude-test\""));
    }

    #[tokio::test]
    async fn static_dir_serves_assets_and_returns_404_for_missing_paths() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("index.html"), "<html>arena</html>").unwrap();
        std::fs::write(dir.path().join("arena.css"), "body {}").unwrap();

        let app = build_router(test_state(), Some(dir.path()));
        let asset = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/arena.css")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(asset.status(), StatusCode::OK);
        assert_eq!(response_text(asset).await, "body {}");
        let missing = app
            .oneshot(
                Request::builder()
                    .uri("/missing")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(missing.status(), StatusCode::NOT_FOUND);
        assert!(!response_text(missing).await.contains("<html>arena</html>"));
    }
}
