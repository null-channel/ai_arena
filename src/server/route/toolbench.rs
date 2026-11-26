use super::private_response;
use crate::server::{
    error::AppError,
    htmx::layout,
    state::AppState,
    toolbench::{Progress, Report, Run, Settings, Status, catalog},
};
use askama::Template;
use axum::{
    Form, Json, Router,
    extract::{Path, State},
    http::StatusCode,
    middleware,
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
};
use uuid::Uuid;

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/toolbench", get(index).post(start))
        .route("/toolbench/models", post(save_model))
        .route("/toolbench/history", get(history))
        .route("/toolbench/{id}", get(detail))
        .route_layer(middleware::from_fn(private_response))
}

#[derive(Template)]
#[template(path = "toolbench.html")]
struct Setup<'a> {
    runs: &'a [Run],
    settings: &'a Settings,
    error: &'a str,
    presets: &'a [String],
    providers: &'a [&'a str],
    catalog_error: String,
}

#[derive(Template)]
#[template(path = "toolbench_history.html")]
struct History<'a> {
    runs: &'a [Run],
}

#[derive(Template)]
#[template(path = "toolbench_run.html")]
struct Detail<'a> {
    run: &'a Run,
    report: Option<Report>,
    running: bool,
    progress: Progress,
    badge: &'a str,
    error: &'a str,
}

async fn setup(
    state: &AppState,
    settings: &Settings,
    error: &str,
) -> Result<Html<String>, AppError> {
    let runs = state.toolbench.history().await.map_err(internal)?;
    let (presets, catalog_error) = match catalog::list(&catalog::path()).await {
        Ok(models) => (models, String::new()),
        Err(error) => (
            Vec::new(),
            format!("Could not read the model list: {error}"),
        ),
    };
    let content = Setup {
        runs: &runs,
        settings,
        error,
        presets: &presets,
        providers: catalog::PROVIDERS,
        catalog_error,
    }
    .render()
    .map_err(internal)?;
    Ok(Html(layout("Tool Calling", &content)))
}

#[derive(serde::Deserialize)]
struct SaveModel {
    provider: String,
    model: String,
}

async fn save_model(Form(form): Form<SaveModel>) -> Result<Json<Vec<String>>, AppError> {
    let model = catalog::model_id(&form.provider, &form.model).map_err(AppError::BadRequest)?;
    let models = catalog::add(catalog::path(), model)
        .await
        .map_err(|e| AppError::Internal(format!("Could not save the model list: {e}")))?;
    Ok(Json(models))
}

async fn index(State(state): State<AppState>) -> Result<Html<String>, AppError> {
    setup(&state, &Settings::default(), "").await
}

async fn history(State(state): State<AppState>) -> Result<Html<String>, AppError> {
    let runs = state.toolbench.history().await.map_err(internal)?;
    Ok(Html(History { runs: &runs }.render().map_err(internal)?))
}

async fn start(
    State(state): State<AppState>,
    Form(settings): Form<Settings>,
) -> Result<Response, AppError> {
    match state.toolbench.start(settings.clone()).await {
        Ok(id) => Ok(Redirect::to(&format!("/toolbench/{id}")).into_response()),
        Err(error) => Ok((
            StatusCode::BAD_REQUEST,
            setup(&state, &settings, &error).await?,
        )
            .into_response()),
    }
}

async fn detail(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Html<String>, AppError> {
    let (run, progress) = state.toolbench.get(id).await.map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            AppError::NotFound("Benchmark run not found".into())
        } else {
            internal(e)
        }
    })?;
    let (badge, error, report) = match &run.status {
        Status::Running => ("running", "", None),
        Status::Completed => (
            "completed",
            "",
            Some(state.toolbench.report(id).await.map_err(internal)?),
        ),
        Status::Failed(message) => ("error", message.as_str(), None),
        Status::Interrupted => (
            "error",
            "The Arena process that started this run is no longer tracking it.",
            None,
        ),
    };
    let content = Detail {
        run: &run,
        progress,
        report,
        running: matches!(run.status, Status::Running),
        badge,
        error,
    }
    .render()
    .map_err(internal)?;
    Ok(Html(layout("Tool Calling", &content)))
}

fn internal(error: impl std::fmt::Display) -> AppError {
    AppError::Internal(error.to_string())
}
