use super::private_response;
use crate::server::jobs::MAX_JOB_RUNS;
use crate::{
    agent_config::build_agent,
    server::{
        config::AgentFormDefaults,
        error::AppError,
        htmx,
        kubebench::{Report, Task},
        state::AppState,
    },
};
use askama::Template;
use axum::{
    Form, Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    middleware,
    response::{Html, IntoResponse, Redirect, Response},
    routing::get,
};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/kubebench", get(index).post(start))
        .route("/kubebench/matrix", get(matrix).post(start_matrix))
        .route("/kubebench/history", get(history))
        .route("/kubebench/{id}", get(detail))
        .route_layer(middleware::from_fn(private_response))
}

#[derive(Template)]
#[template(path = "kubebench.html")]
struct Setup {
    tasks: Vec<Task>,
    matrix: bool,
    limit: usize,
    agent: String,
    agent_template: String,
    recent: Vec<Report>,
    error: String,
    selection: String,
}
#[derive(Template)]
#[template(path = "kubebench_run.html")]
struct Detail {
    run: Report,
}

async fn setup(
    state: &AppState,
    config: &AgentFormDefaults,
    error: String,
    selection: String,
    matrix: bool,
) -> Result<Html<String>, AppError> {
    let profiles = state.secrets.lock().await.list_profiles();
    let (tasks, error) = match state.kubebench.catalog().await {
        Ok(tasks) => (tasks, error),
        Err(reason) => (Vec::new(), reason),
    };
    let mut agent_template = String::new();
    let agent = if matrix {
        let mut blank = config.clone();
        blank.model.clear();
        agent_template = htmx::render_agent("newslot", 2, &blank, &profiles, true);
        htmx::render_agent("slot1", 1, config, &profiles, true)
    } else {
        htmx::render_agent("agent", 1, config, &profiles, false)
    };
    let recent = state
        .kubebench
        .history()
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
        .into_iter()
        .filter(|run| run.matrix == matrix)
        .take(8)
        .collect();
    let content = Setup {
        tasks,
        matrix,
        limit: MAX_JOB_RUNS,
        agent,
        agent_template,
        recent,
        error,
        selection,
    }
    .render()
    .map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(Html(htmx::layout(
        state.mode(),
        "KubeBench",
        "/kubebench",
        &content,
    )))
}
async fn index(State(state): State<AppState>) -> Result<Html<String>, AppError> {
    let config = state.form_defaults.lock().await.agent_one.clone();
    setup(&state, &config, String::new(), "all".into(), false).await
}
#[derive(Deserialize)]
struct Launch {
    suite: String,
    agent_kind: String,
    agent_model: String,
    agent_temp: Option<f32>,
    agent_secret_profile: Option<String>,
    agent_api_endpoint: Option<String>,
    agent_prompt_file: Option<String>,
}
async fn start(
    State(state): State<AppState>,
    Form(form): Form<Launch>,
) -> Result<axum::response::Response, AppError> {
    let mode = state.mode();
    let catalog = state
        .kubebench
        .catalog()
        .await
        .map_err(AppError::BadRequest)?;
    let tasks: Vec<_> = catalog
        .into_iter()
        .filter(|t| form.suite == "all" || t.id == form.suite)
        .collect();
    if tasks.is_empty() {
        return Err(AppError::BadRequest("Unknown KubeBench task.".into()));
    }
    let config = AgentFormDefaults {
        kind: form.agent_kind.parse().map_err(AppError::BadRequest)?,
        model: form.agent_model,
        temp: form.agent_temp.unwrap_or(0.7),
        seed: None,
        secret_profile: form.agent_secret_profile.filter(|s| !s.is_empty()),
        api_endpoint: form.agent_api_endpoint,
        prompt_file: form.agent_prompt_file.filter(|s| !s.is_empty()),
    };
    let agent = {
        let secrets = state.secrets.lock().await;
        config
            .clone()
            .into_config()
            .and_then(|c| build_agent(0, c, &secrets))
    };
    match agent {
        Ok(agent) => {
            let id = state
                .kubebench
                .start(
                    vec![(format!("{:?} / {}", config.kind, config.model), agent)],
                    tasks,
                    mode,
                    false,
                )
                .await
                .map_err(|e| AppError::Internal(e.to_string()))?;
            Ok(Redirect::to(&format!("/kubebench/{id}")).into_response())
        }
        Err(e) => Ok((
            axum::http::StatusCode::BAD_REQUEST,
            setup(&state, &config, e, form.suite, false).await?,
        )
            .into_response()),
    }
}
async fn matrix(State(state): State<AppState>) -> Result<Html<String>, AppError> {
    let config = state.form_defaults.lock().await.agent_one.clone();
    setup(&state, &config, String::new(), String::new(), true).await
}

async fn start_matrix(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(fields): Form<HashMap<String, String>>,
) -> Result<Response, AppError> {
    let mode = state.mode();
    let launch = async {
        let configs = super::matrix::roster_agents(&fields)?;
        let selected: HashSet<_> = fields
            .keys()
            .filter_map(|key| key.strip_prefix("task:"))
            .collect();
        if configs.is_empty() || selected.is_empty() {
            return Err("Select at least one agent and one task.".to_owned());
        }
        if configs.len().saturating_mul(selected.len()) > MAX_JOB_RUNS {
            return Err(format!("Choose at most {MAX_JOB_RUNS} agent–task runs."));
        }
        let tasks: Vec<_> = state
            .kubebench
            .catalog()
            .await?
            .into_iter()
            .filter(|task| selected.contains(task.id.as_str()))
            .collect();
        if tasks.len() != selected.len() {
            return Err(
                "The task catalog changed. Refresh the page to select available tasks.".into(),
            );
        }
        let mut agents = Vec::with_capacity(configs.len());
        let secrets = state.secrets.lock().await;
        for (index, config) in configs.into_iter().enumerate() {
            let label = format!("{:?} / {}", config.agent, config.model);
            let agent = build_agent(index, config, &secrets)
                .map_err(|e| format!("Agent {}: {e}", index + 1))?;
            agents.push((label, agent));
        }
        Ok((agents, tasks))
    }
    .await;
    let (agents, tasks) = match launch {
        Ok(selection) => selection,
        Err(error) => {
            return Ok((
                StatusCode::BAD_REQUEST,
                [("HX-Reswap", "textContent")],
                error,
            )
                .into_response());
        }
    };
    let id = state
        .kubebench
        .start(agents, tasks, mode, true)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    let target = format!("/kubebench/{id}");
    if headers.contains_key("HX-Request") {
        Ok(([("HX-Redirect", target)], "").into_response())
    } else {
        Ok(Redirect::to(&target).into_response())
    }
}

#[derive(Template)]
#[template(path = "kubebench_history.html")]
struct History {
    runs: Vec<Report>,
}

async fn history(State(state): State<AppState>) -> Result<Html<String>, AppError> {
    let runs = state
        .kubebench
        .history()
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    let content = History { runs }
        .render()
        .map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(Html(htmx::layout(
        state.mode(),
        "KubeBench · Run history",
        "/kubebench/history",
        &content,
    )))
}

async fn detail(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Html<String>, AppError> {
    let run = state.kubebench.get(id).await.map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            AppError::NotFound("KubeBench run not found.".into())
        } else {
            AppError::Internal(e.to_string())
        }
    })?;
    let content = Detail { run }
        .render()
        .map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(Html(htmx::layout(
        state.mode(),
        "KubeBench",
        "/kubebench/history",
        &content,
    )))
}
