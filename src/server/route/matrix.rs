use crate::agent_config::AIAgentConfig;
use crate::server::{
    error::AppError,
    htmx,
    jobs::{CreateMatrixRequest, MatrixJobResponse, create_matrix_job, job_summaries},
    state::AppState,
};
use axum::{
    Form, Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::{Html, IntoResponse, Response},
    routing::{get, post},
};
use serde::Deserialize;

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/matrix", get(matrix_fragment))
        .route("/matrix/start", post(start_matrix_from_form))
        .route("/api/matrix", post(create_matrix))
        .route("/api/matrix/{id}", get(get_matrix_status))
}

#[derive(Debug, Clone, Deserialize)]
struct MatrixForm {
    game_type: String,
    repetitions: u32,
    concurrency_limit: usize,
    agent_one_kind: String,
    agent_one_model: String,
    agent_one_temp: f32,
    agent_two_kind: String,
    agent_two_model: String,
    agent_two_temp: f32,
}

impl MatrixForm {
    fn into_request(self) -> Result<CreateMatrixRequest, String> {
        Ok(CreateMatrixRequest {
            game_type: self.game_type,
            agents: vec![
                AIAgentConfig {
                    agent: self.agent_one_kind.parse()?,
                    model: self.agent_one_model,
                    temp: self.agent_one_temp,
                    seed: None,
                    secret_profile: None,
                    api_endpoint: None,
                    prompt_file: None,
                },
                AIAgentConfig {
                    agent: self.agent_two_kind.parse()?,
                    model: self.agent_two_model,
                    temp: self.agent_two_temp,
                    seed: None,
                    secret_profile: None,
                    api_endpoint: None,
                    prompt_file: None,
                },
            ],
            repetitions: self.repetitions,
            concurrency_limit: Some(self.concurrency_limit),
        })
    }
}

async fn matrix_fragment(State(state): State<AppState>) -> Html<String> {
    Html(htmx::render_matrix_fragment(&job_summaries(&state).await))
}

async fn start_matrix_from_form(
    State(state): State<AppState>,
    Form(form): Form<MatrixForm>,
) -> Response {
    let defaults = state.form_defaults.lock().await.clone();
    match form.into_request() {
        Ok(request) => match create_matrix_job(&state, request).await {
            Ok(job) => {
                let matrix = htmx::render_matrix_fragment(&job_summaries(&state).await);
                Html(htmx::render_matrix_started(&job, &matrix)).into_response()
            }
            Err(error) => error.into_response(),
        },
        Err(error) => (
            StatusCode::BAD_REQUEST,
            Html(htmx::render_matrix_form(&defaults, Some(&error))),
        )
            .into_response(),
    }
}

async fn create_matrix(
    State(state): State<AppState>,
    Json(request): Json<CreateMatrixRequest>,
) -> Result<Json<MatrixJobResponse>, AppError> {
    Ok(Json(create_matrix_job(&state, request).await?))
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
