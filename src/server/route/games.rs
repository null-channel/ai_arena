use crate::server::{
    config::{AgentFormDefaults, FormDefaults},
    error::AppError,
    htmx,
    sessions::{GameStatus, create_game_session, game_detail, live_game_summaries, run_session},
    state::AppState,
};
use crate::{
    agent_config::{AIAgentConfig, AgentKind},
    games::{events::GameEvent, game::TestResult},
};
use axum::{
    Form, Json, Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/games", get(games_fragment))
        .route(
            "/games/start",
            get(start_form_fragment).post(start_game_from_form),
        )
        .route("/games/defaults", post(update_game_defaults))
        .route("/games/{id}", get(game_page))
        .route("/games/{id}/fragment", get(game_fragment))
        .route("/api/games", get(list_games).post(create_game))
        .route("/api/games/{id}", get(get_game_state))
}

#[derive(Debug, Deserialize)]
pub struct CreateGameRequest {
    pub game_type: String,
    pub agents: Vec<AIAgentConfig>,
}

#[derive(Debug, Serialize)]
pub struct CreateGameResponse {
    pub game_id: String,
}

#[derive(Debug, Serialize)]
pub struct GameListItem {
    pub game_id: String,
    pub game_type: String,
    pub status: GameStatus,
}

#[derive(Debug, Serialize)]
pub struct GameStateResponse {
    pub game_id: String,
    pub status: GameStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_event: Option<GameEvent>,
    pub result: Option<TestResult>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StartGameForm {
    pub(super) repetitions: Option<u32>,
    pub(super) concurrency_limit: Option<usize>,
    game_type: String,
    agent_one_kind: String,
    agent_one_model: String,
    agent_one_temp: Option<f32>,
    #[serde(default, deserialize_with = "optional_seed")]
    agent_one_seed: Option<u64>,
    #[serde(default)]
    agent_one_secret_profile: Option<String>,
    #[serde(default)]
    agent_one_api_endpoint: Option<String>,
    #[serde(default)]
    agent_one_prompt_file: Option<String>,
    agent_two_kind: String,
    agent_two_model: String,
    agent_two_temp: Option<f32>,
    #[serde(default, deserialize_with = "optional_seed")]
    agent_two_seed: Option<u64>,
    #[serde(default)]
    agent_two_secret_profile: Option<String>,
    #[serde(default)]
    agent_two_api_endpoint: Option<String>,
    #[serde(default)]
    agent_two_prompt_file: Option<String>,
}

impl StartGameForm {
    pub(super) fn into_request(self) -> Result<CreateGameRequest, String> {
        self.agent_one_kind.parse::<AgentKind>()?;
        self.agent_two_kind.parse::<AgentKind>()?;
        let defaults = self.to_form_defaults();
        let agents = vec![
            defaults.agent_one.into_config()?,
            defaults.agent_two.into_config()?,
        ];
        Ok(CreateGameRequest {
            game_type: defaults.game_type,
            agents,
        })
    }

    pub(super) fn to_form_defaults(&self) -> FormDefaults {
        FormDefaults {
            game_type: self.game_type.clone(),
            agent_one: AgentFormDefaults {
                kind: self.agent_one_kind.parse().unwrap_or(AgentKind::OpenAI),
                model: self.agent_one_model.clone(),
                temp: self.agent_one_temp.unwrap_or(0.7),
                seed: self.agent_one_seed,
                secret_profile: clean_optional(self.agent_one_secret_profile.clone()),
                api_endpoint: clean_optional(self.agent_one_api_endpoint.clone()),
                prompt_file: clean_optional(self.agent_one_prompt_file.clone()),
            },
            agent_two: AgentFormDefaults {
                kind: self.agent_two_kind.parse().unwrap_or(AgentKind::Ollama),
                model: self.agent_two_model.clone(),
                temp: self.agent_two_temp.unwrap_or(0.7),
                seed: self.agent_two_seed,
                secret_profile: clean_optional(self.agent_two_secret_profile.clone()),
                api_endpoint: clean_optional(self.agent_two_api_endpoint.clone()),
                prompt_file: clean_optional(self.agent_two_prompt_file.clone()),
            },
        }
    }
}

fn optional_seed<'de, D: serde::Deserializer<'de>>(field: D) -> Result<Option<u64>, D::Error> {
    let value = String::deserialize(field)?;
    if value.is_empty() {
        Ok(None)
    } else {
        value.parse().map(Some).map_err(serde::de::Error::custom)
    }
}

fn clean_optional(value: Option<String>) -> Option<String> {
    value.and_then(|v| {
        let trimmed = v.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

async fn start_form_fragment(State(state): State<AppState>) -> Html<String> {
    let secrets = state.secrets.lock().await;
    let profiles = secrets.list_profiles();
    drop(secrets);
    let defaults = state.form_defaults.lock().await.clone();
    Html(htmx::render_start_form(&profiles, &defaults, None))
}

async fn games_fragment(State(state): State<AppState>) -> Html<String> {
    Html(htmx::render_games_fragment(
        &live_game_summaries(&state).await,
    ))
}

async fn game_page(
    State(state): State<AppState>,
    Path(game_id): Path<String>,
) -> Result<Html<String>, AppError> {
    let (game, events) = game_detail(&state, &game_id).await?;
    let fragment = htmx::render_game_fragment(&game, &events)?;
    let parent = state
        .game_store(&game_id)
        .await
        .parent_batch(&game_id)
        .await
        .map_err(AppError::Internal)?;
    Ok(Html(htmx::render_game_page(
        state.mode(),
        &fragment,
        parent,
    )))
}

async fn game_fragment(
    State(state): State<AppState>,
    Path(game_id): Path<String>,
) -> Result<Html<String>, AppError> {
    let (game, events) = game_detail(&state, &game_id).await?;
    Ok(Html(htmx::render_game_fragment(&game, &events)?))
}

async fn start_game_from_form(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<StartGameForm>,
) -> Response {
    let submitted_defaults = form.to_form_defaults();
    let message = match form.into_request() {
        Ok(request) => match create_game_session(&state, request.game_type, request.agents).await {
            Ok(game_id) => {
                *state.form_defaults.lock().await = submitted_defaults;
                tokio::spawn(run_session(state.clone(), game_id.clone()));
                let path = format!("/games/{game_id}");
                return if headers.contains_key("HX-Request") {
                    ([("HX-Redirect", path)], "").into_response()
                } else {
                    Redirect::to(&path).into_response()
                };
            }
            Err(AppError::BadRequest(message)) => message,
            Err(error) => return error.into_response(),
        },
        Err(message) => message,
    };
    let profiles = state.secrets.lock().await.list_profiles();
    (
        StatusCode::BAD_REQUEST,
        Html(htmx::render_start_form(
            &profiles,
            &submitted_defaults,
            Some(&message),
        )),
    )
        .into_response()
}

async fn update_game_defaults(
    State(state): State<AppState>,
    Form(form): Form<StartGameForm>,
) -> StatusCode {
    *state.form_defaults.lock().await = form.to_form_defaults();
    StatusCode::NO_CONTENT
}

async fn list_games(State(state): State<AppState>) -> Json<Vec<GameListItem>> {
    let games = state.games.lock().await;
    let mut items = games
        .iter()
        .map(|(game_id, session)| GameListItem {
            game_id: game_id.clone(),
            game_type: session.game_type.clone(),
            status: session.status.clone(),
        })
        .collect::<Vec<_>>();
    let active_ids = games
        .keys()
        .cloned()
        .collect::<std::collections::HashSet<_>>();
    drop(games);

    match state.store.list_runs(None).await {
        Ok(stored_runs) => {
            items.extend(stored_runs.into_iter().filter_map(|run| {
                (!active_ids.contains(&run.id)).then(|| GameListItem {
                    game_id: run.id,
                    game_type: run.game_type,
                    status: GameStatus::from_storage(&run.status),
                })
            }));
        }
        Err(error) => eprintln!("Failed to list DuckDB runs: {}", error),
    }

    Json(items)
}

async fn create_game(
    State(state): State<AppState>,
    Json(request): Json<CreateGameRequest>,
) -> Result<Json<CreateGameResponse>, AppError> {
    println!("API: Received CreateGameRequest: {:?}", request);
    let game_id = create_game_session(&state, request.game_type, request.agents).await?;

    Ok(Json(CreateGameResponse { game_id }))
}

async fn get_game_state(
    State(state): State<AppState>,
    Path(game_id): Path<String>,
) -> Result<Json<GameStateResponse>, AppError> {
    let (game, events) = game_detail(&state, &game_id).await?;
    Ok(Json(GameStateResponse {
        game_id,
        status: game.status,
        latest_event: events.last().cloned(),
        result: game.result,
    }))
}

#[cfg(test)]
mod tests {
    use crate::agent_config::{AIAgentConfig, AgentKind};
    use crate::games::{events::GameEvent, game::Game};
    use crate::server::{
        route::build_router,
        sessions::{GameSession, GameStatus},
        test::{response_text, test_state},
    };
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use serde_json::json;
    use tower::ServiceExt;

    #[tokio::test]
    async fn defaults_endpoint_updates_next_form_render() {
        let app = build_router(test_state(), None);
        let body = Body::from(
            "game_type=Chess&agent_one_kind=Ollama&agent_one_model=llama-updated&agent_one_temp=0.3&agent_one_seed=11&agent_one_api_endpoint=http%3A%2F%2F127.0.0.1%3A11434&agent_two_kind=Gemini&agent_two_model=gemini-updated&agent_two_temp=0.5&agent_two_seed=12&agent_two_secret_profile=gemini.dev",
        );
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/games/defaults")
                    .header("content-type", "application/x-www-form-urlencoded")
                    .body(body)
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/games/start")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body = response_text(response).await;
        assert!(body.contains("value=\"Chess\" checked"));
        assert!(body.contains("value=\"llama-updated\""));
        assert!(body.contains("value=\"gemini-updated\""));
    }

    #[tokio::test]
    async fn start_form_validation_returns_bad_request_fragment() {
        let app = build_router(test_state(), None);
        let body = Body::from(
            "game_type=TicTacToe&agent_one_kind=BadProvider&agent_one_model=m1&agent_one_temp=0.7&agent_two_kind=Ollama&agent_two_model=m2&agent_two_temp=0.7",
        );
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/games/start")
                    .header("content-type", "application/x-www-form-urlencoded")
                    .body(body)
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = response_text(response).await;
        assert!(body.contains("Could not start match"));
        assert!(body.contains("Unsupported agent kind"));
    }

    #[tokio::test]
    async fn start_game_redirects_to_observation_page() {
        let app = build_router(test_state(), None);
        let body = Body::from(
            "game_type=TicTacToe&agent_one_kind=Ollama&agent_one_model=m1&agent_one_temp=0.7&agent_one_seed=&agent_one_api_endpoint=http%3A%2F%2F127.0.0.1%3A11434&agent_two_kind=Ollama&agent_two_model=m2&agent_two_temp=0.7&agent_two_seed=2&agent_two_api_endpoint=http%3A%2F%2F127.0.0.1%3A11434",
        );
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/games/start")
                    .header("HX-Request", "true")
                    .header("content-type", "application/x-www-form-urlencoded")
                    .body(body)
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let redirect = response
            .headers()
            .get("HX-Redirect")
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default();
        assert!(redirect.starts_with("/games/"));
    }

    #[tokio::test]
    async fn running_game_fragment_renders_latest_observer_state() {
        let state = test_state();
        let game_id = "live-game".to_string();
        state.games.lock().await.insert(
            game_id.clone(),
            GameSession {
                mode: crate::server::state::ArenaMode::Test,
                game_type: "TicTacToe".to_string(),
                game: Game::new("TicTacToe").unwrap(),
                agents: Vec::new(),
                status: GameStatus::Running,
                events: vec![
                    GameEvent::turn(
                        "TicTacToe",
                        1,
                        json!({"note": "<script>first turn</script>"}),
                    ),
                    GameEvent::turn(
                        "TicTacToe",
                        2,
                        json!({
                            "board": [["X", null, null], [null, "O", null], [null, null, null]],
                            "current_player": "X",
                            "turn_number": 2
                        }),
                    ),
                ]
                .into(),
                result: None,
            },
        );

        let agents = vec![
            AIAgentConfig {
                agent: AgentKind::OpenAI,
                model: "gpt-test".to_string(),
                temp: 0.7,
                seed: None,
                secret_profile: None,
                api_endpoint: None,
                prompt_file: None,
            },
            AIAgentConfig {
                agent: AgentKind::Ollama,
                model: "qwen3.6:latest".to_string(),
                temp: 0.7,
                seed: None,
                secret_profile: None,
                api_endpoint: None,
                prompt_file: None,
            },
        ];
        state.games.lock().await.get_mut(&game_id).unwrap().agents = agents;

        let response = build_router(state, None)
            .oneshot(
                Request::builder()
                    .uri(format!("/games/{game_id}/fragment"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let html = response_text(response).await;
        assert!(html.contains("plays X"));
        assert!(html.contains("gpt-test"));
        assert!(html.contains("Turn 1"));
        assert!(html.contains("Turn 2"));
        assert!(html.contains("first turn"));
        assert!(!html.contains("<script>first turn</script>"));
    }

    #[tokio::test]
    async fn game_page_polls_every_500ms() {
        let state = test_state();
        let game_id = "poll-game".to_string();
        state.games.lock().await.insert(
            game_id.clone(),
            GameSession {
                mode: crate::server::state::ArenaMode::Test,
                game_type: "TicTacToe".to_string(),
                game: Game::new("TicTacToe").unwrap(),
                agents: Vec::new(),
                status: GameStatus::Running,
                events: Default::default(),
                result: None,
            },
        );
        let app = build_router(state, None);

        let response = app
            .oneshot(
                Request::builder()
                    .uri(format!("/games/{}", game_id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = response_text(response).await;
        assert_eq!(body.matches("hx-trigger=\"every 500ms\"").count(), 1);
        assert!(body.contains("Waiting for the first turn."));
    }

    #[tokio::test]
    async fn create_game_invalid_type_returns_400() {
        let app = build_router(test_state(), None);
        let body = Body::from(
            r#"{"game_type":"NotAGame","agents":[{"agent":"OpenAI","model":"gpt-4","temp":0.7}]}"#,
        );
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/games")
                    .header("content-type", "application/json")
                    .body(body)
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn create_game_returns_game_id() {
        let app = build_router(test_state(), None);
        let body = Body::from(
            r#"{"game_type":"TicTacToe","agents":[
                {"agent":"OpenAI","model":"gpt-4","temp":0.7},
                {"agent":"OpenAI","model":"gpt-4","temp":0.7}
            ]}"#,
        );
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/games")
                    .header("content-type", "application/json")
                    .body(body)
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn list_games_includes_created_session() {
        let state = test_state();
        let app = build_router(state.clone(), None);

        let create_body = Body::from(
            r#"{"game_type":"TicTacToe","agents":[
                {"agent":"OpenAI","model":"gpt-4","temp":0.7},
                {"agent":"OpenAI","model":"gpt-4","temp":0.7}
            ]}"#,
        );
        let create_resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/games")
                    .header("content-type", "application/json")
                    .body(create_body)
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(create_resp.status(), StatusCode::OK);

        let list_resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/games")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(list_resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn games_fragment_lists_created_session() {
        let state = test_state();
        let app = build_router(state.clone(), None);
        let create_body = Body::from(
            r#"{"game_type":"ConnectFour","agents":[
                {"agent":"OpenAI","model":"gpt-4","temp":0.7},
                {"agent":"OpenAI","model":"gpt-4","temp":0.7}
            ]}"#,
        );
        let create_resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/games")
                    .header("content-type", "application/json")
                    .body(create_body)
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(create_resp.status(), StatusCode::OK);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/games")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = response_text(response).await;
        assert!(body.contains("ConnectFour"));
        assert!(body.contains("Observe"));
    }
}
