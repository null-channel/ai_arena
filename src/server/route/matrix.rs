use super::games::StartGameForm;
use crate::server::{
    config::{AgentFormDefaults, RunMode},
    error::AppError,
    htmx,
    jobs::{CreateMatrixRequest, MatrixJobResponse, create_matrix_job},
    state::AppState,
};
use axum::{
    Extension, Form, Json, Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
};
use std::collections::HashMap;

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .nest("/multi", batch_routes(RunMode::Multi))
        .nest("/matrix", batch_routes(RunMode::Matrix))
        .route("/matrix/start", post(start_matrix_from_form))
        .route("/round-robin/start", post(start_round_robin))
        .route("/api/matrix", post(create_matrix))
        .route("/api/matrix/{id}", get(get_matrix_status))
}

fn batch_routes(mode: RunMode) -> Router<AppState> {
    let setup = get(batch_setup);
    let setup = match mode {
        RunMode::Multi => setup.post(start_matrix_from_form),
        RunMode::Matrix => setup.post(start_round_robin),
        RunMode::Single => unreachable!("single runs use the game routes"),
    };
    Router::new()
        .route("/", setup)
        .route("/activity", get(batch_activity))
        .route("/results", get(super::runs::recent_results))
        .route("/{id}", get(batch_page))
        .layer(Extension(mode))
}

async fn batch_setup(
    State(state): State<AppState>,
    Extension(mode): Extension<RunMode>,
) -> Html<String> {
    super::router::console_page(state, mode).await
}

pub(super) async fn batch_activity(
    State(state): State<AppState>,
    Extension(mode): Extension<RunMode>,
) -> Result<Html<String>, AppError> {
    let batches = state
        .store
        .list_batches(mode.path().trim_start_matches('/'))
        .await
        .map_err(AppError::Internal)?;
    Ok(Html(htmx::render_batch_activity(mode, &batches)))
}

async fn batch_page(
    State(state): State<AppState>,
    Extension(mode): Extension<RunMode>,
    Path(id): Path<String>,
) -> Result<Html<String>, AppError> {
    let sessions = state
        .store
        .batch_sessions(mode.path().trim_start_matches('/'), &id)
        .await
        .map_err(AppError::Internal)?;
    let first = sessions
        .first()
        .ok_or_else(|| AppError::NotFound("Batch not found".into()))?;
    let active = state.matrix_jobs.lock().await.contains_key(&id);
    Ok(Html(htmx::render_batch_page(
        mode,
        &id,
        &first.game_type,
        &sessions,
        active,
    )))
}

fn open_batch(headers: &HeaderMap, mode: RunMode, id: &str) -> Response {
    let path = format!("{}/{id}", mode.path());
    if headers.contains_key("HX-Request") {
        ([("HX-Redirect", path)], "").into_response()
    } else {
        Redirect::to(&path).into_response()
    }
}

async fn start_matrix_from_form(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<StartGameForm>,
) -> Response {
    let defaults = form.to_form_defaults();
    let repetitions = form.repetitions.unwrap_or(10);
    let concurrency = form.concurrency_limit.unwrap_or(4);
    let request = form.into_request().and_then(|game| {
        if !(1..=32).contains(&concurrency) || !(1..=1000).contains(&repetitions) {
            return Err("Choose 1–1000 matches and 1–32 concurrent matches.".into());
        }
        Ok(CreateMatrixRequest {
            game_type: game.game_type,
            agents: game.agents,
            repetitions,
            concurrency_limit: Some(concurrency),
        })
    });
    let message = match request {
        Ok(request) => match create_matrix_job(&state, request, RunMode::Multi).await {
            Ok(job) => {
                *state.form_defaults.lock().await = defaults;
                return open_batch(&headers, RunMode::Multi, &job.matrix_id);
            }
            Err(AppError::BadRequest(error)) => error,
            Err(error) => return error.into_response(),
        },
        Err(error) => error,
    };
    let profiles = state.secrets.lock().await.list_profiles();
    (
        StatusCode::BAD_REQUEST,
        Html(htmx::render_form(
            &profiles,
            &defaults,
            RunMode::Multi,
            Some((repetitions, concurrency)),
            Some(&message),
        )),
    )
        .into_response()
}

// A roster can contain any number of cards. Each card submits its own numbered
// fields, so omitting an unsupported setting cannot shift another agent's values.
fn roster_request(fields: &HashMap<String, String>) -> Result<CreateMatrixRequest, String> {
    let get = |key: &str| fields.get(key).map(|v| v.trim()).filter(|v| !v.is_empty());
    let repetitions = get("repetitions")
        .and_then(|v| v.parse().ok())
        .ok_or("Enter the number of matches per pair.")?;
    let concurrency = get("concurrency_limit")
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|v| (1..=32).contains(v))
        .ok_or("Choose 1–32 concurrent matches.")?;
    let mut slots: Vec<usize> = fields
        .keys()
        .filter_map(|key| {
            key.strip_prefix("slot")?
                .strip_suffix("_kind")?
                .parse()
                .ok()
        })
        .collect();
    slots.sort_unstable();
    let mut agents = Vec::with_capacity(slots.len());
    for slot in slots {
        let field = |name: &str| get(&format!("slot{slot}_{name}"));
        let config = AgentFormDefaults {
            kind: field("kind")
                .ok_or("Choose a provider for every agent.")?
                .parse()?,
            model: field("model").unwrap_or_default().to_owned(),
            temp: field("temp")
                .map(str::parse)
                .transpose()
                .map_err(|_| "Enter a numeric temperature.")?
                .unwrap_or(0.7),
            api_endpoint: field("api_endpoint").map(str::to_owned),
            secret_profile: field("secret_profile").map(str::to_owned),
            prompt_file: field("prompt_file").map(str::to_owned),
            seed: None,
        };
        agents.push(config.into_config()?);
    }
    Ok(CreateMatrixRequest {
        game_type: get("game_type").ok_or("Select a game.")?.to_owned(),
        repetitions,
        concurrency_limit: Some(concurrency),
        agents,
    })
}

async fn start_round_robin(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(fields): Form<HashMap<String, String>>,
) -> Response {
    let result = match roster_request(&fields) {
        Ok(request) => create_matrix_job(&state, request, RunMode::Matrix).await,
        Err(error) => Err(AppError::BadRequest(error)),
    };
    match result {
        Ok(job) => open_batch(&headers, RunMode::Matrix, &job.matrix_id),
        Err(AppError::BadRequest(error)) => (
            StatusCode::BAD_REQUEST,
            [("HX-Retarget", "#matrix-feedback")],
            // Plain text is escaped by the HTMX textContent swap.
            [("HX-Reswap", "textContent")],
            error,
        )
            .into_response(),
        Err(error) => error.into_response(),
    }
}

async fn create_matrix(
    State(state): State<AppState>,
    Json(request): Json<CreateMatrixRequest>,
) -> Result<Json<MatrixJobResponse>, AppError> {
    Ok(Json(
        create_matrix_job(&state, request, RunMode::Matrix).await?,
    ))
}

async fn get_matrix_status(
    State(state): State<AppState>,
    Path(matrix_id): Path<String>,
) -> Result<Json<MatrixJobResponse>, AppError> {
    let jobs = state.matrix_jobs.lock().await;
    let job = jobs
        .get(&matrix_id)
        .ok_or_else(|| AppError::NotFound("Matrix job not found".to_string()))?;
    Ok(Json(job.response()))
}

#[cfg(test)]
mod tests {
    use crate::{
        games::game::Game,
        server::{
            route::build_router,
            test::{response_text, test_state},
        },
        storage::RunMetadata,
    };
    use axum::{
        Router,
        body::Body,
        http::{Request, StatusCode},
        response::Response,
    };
    use tower::ServiceExt;

    async fn get(app: &Router, path: &str) -> Response {
        app.clone()
            .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn recent_results_follow_the_mode_and_selected_game() {
        let state = test_state();
        let game = Game::from("TicTacToe");
        let single = state.store.create_run(&game, &[], None).await.unwrap();
        state.store.mark_error(&single, "offline").await.unwrap();

        for (game, mode, batch, finished) in [
            ("TicTacToe", "multi", "series-a", 2),
            ("TicTacToe", "matrix", "grid-b", 2),
            ("TicTacToe", "matrix", "in-progress", 1),
            ("Chess", "matrix", "chess-grid", 2),
        ] {
            let mut meta = RunMetadata::source(mode);
            meta.matrix_job_id = Some(batch.into());
            let ids = state
                .store
                .create_runs(&Game::from(game), vec![(vec![], Some(meta)); 2])
                .await
                .unwrap();
            for id in ids.iter().take(finished) {
                state.store.mark_error(id, "offline").await.unwrap();
            }
        }

        let app = build_router(state, None);
        let single_link = format!("/games/{single}");
        let links = [
            &single_link[..],
            "/multi/series-a",
            "/matrix/grid-b",
            "/matrix/chess-grid",
        ];
        for (index, path) in [
            "/history/summary",
            "/multi/results",
            "/matrix/results",
            "/matrix/results?game_type=Chess",
        ]
        .iter()
        .enumerate()
        {
            let response = get(&app, path).await;
            assert_eq!(response.status(), StatusCode::OK);
            let html = response_text(response).await;
            assert!(!html.contains("in-progress"));
            for (i, link) in links.iter().enumerate() {
                assert_eq!(
                    html.matches(link).count(),
                    usize::from(i == index),
                    "{path} / {link}"
                );
            }
        }
    }

    #[tokio::test]
    async fn saved_batches_link_to_their_own_sessions_and_back() {
        let state = test_state();
        let game = Game::new("TicTacToe").unwrap();
        let mut sessions = Vec::new();
        for (mode, id, count) in [("multi", "a", 2), ("multi", "b", 1), ("matrix", "c", 3)] {
            let runs = (1..=count)
                .map(|round| {
                    let mut meta = RunMetadata::source(mode);
                    meta.matrix_job_id = Some(id.into());
                    meta.repetition = Some(round);
                    (Vec::new(), Some(meta))
                })
                .collect();
            sessions.push(state.store.create_runs(&game, runs).await.unwrap());
        }

        // No jobs are in memory, as when reopening the console after a restart.
        let app = build_router(state, None);
        for (index, (mode, id)) in [("multi", "a"), ("multi", "b"), ("matrix", "c")]
            .iter()
            .enumerate()
        {
            let path = format!("/{mode}/{id}");
            let overview = response_text(get(&app, &format!("/{mode}")).await).await;
            assert!(overview.contains(&format!("href=\"{path}\"")));
            let page = get(&app, &path).await;
            assert_eq!(page.status(), StatusCode::OK);
            let page = response_text(page).await;
            for (batch_index, ids) in sessions.iter().enumerate() {
                for run in ids {
                    assert_eq!(
                        page.contains(&format!("href=\"/games/{run}\"")),
                        index == batch_index
                    );
                }
            }
            let game =
                response_text(get(&app, &format!("/games/{}", sessions[index][0])).await).await;
            assert!(game.contains(&format!("href=\"{path}\"")));
        }
        assert_eq!(get(&app, "/matrix/a").await.status(), StatusCode::NOT_FOUND);
        assert_eq!(
            get(&app, "/multi/missing").await.status(),
            StatusCode::NOT_FOUND
        );
    }
}
