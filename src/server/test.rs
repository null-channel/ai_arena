use super::{config::FormDefaults, route::build_router, state::AppState};
use crate::{secrets::SecretsManager, storage::RunStore};
use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
    response::Response,
};
use std::path::PathBuf;
use tower::ServiceExt;
use uuid::Uuid;

fn test_or_default_db_path() -> PathBuf {
    std::env::temp_dir().join(format!("ai_arena_server_test_{}.duckdb", Uuid::new_v4()))
}

pub(super) fn test_state() -> AppState {
    test_state_with(FormDefaults::default(), None)
}

pub(super) fn test_state_with(defaults: FormDefaults, auth_token: Option<String>) -> AppState {
    AppState::with_store_and_auth(
        SecretsManager::load_from_path(std::path::Path::new("/dev/null")).expect("empty secrets"),
        defaults,
        RunStore::new(test_or_default_db_path()),
        auth_token,
    )
}

pub(super) async fn response_text(response: Response) -> String {
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    String::from_utf8(bytes.to_vec()).unwrap()
}

#[tokio::test]
async fn persisted_run_api_and_replay_page_are_available() {
    let state = test_state();
    state.set_mode(super::state::ArenaMode::Real);
    let app = build_router(state, None);
    let created = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/games")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"game_type":"TicTacToe","agents":[
                    {"agent":"OpenAI","model":"player-one","temp":0.7},
                    {"agent":"OpenAI","model":"player-two","temp":0.7}
                ]}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::OK);
    let game: serde_json::Value = serde_json::from_str(&response_text(created).await).unwrap();
    let run_id = game["game_id"].as_str().unwrap();

    let run_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/runs/{run_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(run_response.status(), StatusCode::OK);
    assert!(response_text(run_response).await.contains(run_id));

    let events_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/runs/{run_id}/events"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(events_response.status(), StatusCode::OK);
    assert_eq!(response_text(events_response).await, "[]");

    let replay_response = app
        .oneshot(
            Request::builder()
                .uri(format!("/games/{run_id}/replay"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(replay_response.status(), StatusCode::OK);
    assert!(
        response_text(replay_response)
            .await
            .contains("This run has no recorded turns.")
    );
}

#[tokio::test]
async fn mode_switches_apply_to_new_games_and_reset_at_server_start() {
    use super::{sessions::create_game_session, state::ArenaMode};
    let state = test_state();
    assert_eq!(state.mode(), ArenaMode::Test);
    let practice = create_game_session(&state, "TicTacToe".into(), vec![])
        .await
        .unwrap();
    let app = build_router(state.clone(), None);
    let response = app
        .oneshot(
            Request::post("/api/mode")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from("mode=real"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let official = create_game_session(&state, "TicTacToe".into(), vec![])
        .await
        .unwrap();
    state.set_mode(ArenaMode::Test);
    for id in [&practice, &official] {
        state
            .game_store(id)
            .await
            .mark_error(id, "fixture")
            .await
            .unwrap();
    }
    let restarted = AppState::with_store_and_auth(
        state.secrets.lock().await.clone(),
        FormDefaults::default(),
        state.store.clone(),
        None,
    );
    assert_eq!(restarted.mode(), ArenaMode::Test);
    assert!(restarted.store.get_run(&practice).await.unwrap().is_none());
    assert_eq!(
        restarted
            .store
            .get_run(&official)
            .await
            .unwrap()
            .unwrap()
            .status,
        "error"
    );
    assert_eq!(
        state
            .test_store
            .get_run(&practice)
            .await
            .unwrap()
            .unwrap()
            .status,
        "error"
    );
}
