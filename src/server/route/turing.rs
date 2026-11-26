use super::private_response;
use crate::server::{
    error::AppError,
    htmx::layout,
    state::AppState,
    turing::{ChatSettings, DEFAULT_URL, Sample, Score, Stage},
};
use askama::Template;
use axum::{
    Form, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    middleware,
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
};
use serde::Deserialize;
use uuid::Uuid;

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/turing", get(index).post(start))
        .route("/turing/chat", post(chat))
        .route("/turing/screen", get(screen))
        .route("/turing/stage", get(stage))
        .route("/turing/scores", get(scores))
        .route("/turing/{id}/media", get(media))
        .route("/turing/{id}/cancel", post(cancel))
        .route_layer(middleware::from_fn(private_response))
}

#[derive(Template)]
#[template(path = "turing.html")]
struct Operator<'a> {
    samples: Vec<Sample>,
    stage: Stage,
    host: bool,
    error: &'a str,
    setup: Setup,
    chat: ChatPanel,
}

#[derive(Template)]
#[template(path = "turing_stage.html")]
struct StagePanel {
    stage: Stage,
    host: bool,
}

#[derive(Template)]
#[template(path = "turing_scores.html")]
struct Standings {
    rows: Vec<Score>,
    lowest: bool,
    total: u64,
    correct: u64,
}

fn render(t: impl Template) -> Result<String, AppError> {
    t.render().map_err(|e| AppError::Internal(e.to_string()))
}

#[derive(Template)]
#[template(path = "turing_chat.html")]
struct ChatPanel {
    url: String,
    message: String,
    failed: bool,
    open: bool,
}

impl ChatPanel {
    async fn current(open: bool) -> Self {
        match ChatSettings::load().await {
            Ok(settings) => Self {
                url: settings.url,
                message: String::new(),
                failed: false,
                open,
            },
            Err(error) => Self {
                url: DEFAULT_URL.into(),
                message: error,
                failed: true,
                open: true,
            },
        }
    }
}

async fn dashboard(
    state: &AppState,
    setup: Setup,
    chat: ChatPanel,
    message: Option<String>,
) -> Result<Html<String>, AppError> {
    let (samples, error) = match state.turing.samples().await {
        Ok(list) => (list, message.unwrap_or_default()),
        Err(e) => (vec![], message.unwrap_or(e)),
    };
    Ok(Html(layout(
        "Turing Test",
        &render(Operator {
            samples,
            stage: state.turing.stage().await,
            host: true,
            error: &error,
            setup,
            chat,
        })?,
    )))
}

async fn index(State(state): State<AppState>) -> Result<Html<String>, AppError> {
    dashboard(
        &state,
        Setup::default(),
        ChatPanel::current(false).await,
        None,
    )
    .await
}

#[derive(Deserialize)]
struct Setup {
    artifact: String,
    review: u32,
    voting: u32,
}

impl Default for Setup {
    fn default() -> Self {
        Self {
            artifact: String::new(),
            review: 20,
            voting: 30,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum ChatAction {
    Test,
    Save,
}

#[derive(Deserialize)]
struct ChatForm {
    url: String,
    action: ChatAction,
}

async fn chat(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<ChatForm>,
) -> Result<Html<String>, AppError> {
    let (url, result) = match ChatSettings::new(&form.url) {
        Ok(settings) => {
            let result = match form.action {
                ChatAction::Test => state.turing.test_chat(&settings).await,
                ChatAction::Save => settings
                    .save()
                    .await
                    .map(|()| "Service URL saved.".to_owned()),
            };
            (settings.url, result)
        }
        Err(error) => (ChatPanel::current(true).await.url, Err(error)),
    };
    let failed = result.is_err();
    let panel = ChatPanel {
        url,
        message: result.unwrap_or_else(|error| error),
        failed,
        open: true,
    };
    if headers.get("hx-request").is_some_and(|v| v == "true") {
        Ok(Html(render(panel)?))
    } else {
        dashboard(&state, Setup::default(), panel, None).await
    }
}

async fn start(
    State(state): State<AppState>,
    Form(form): Form<Setup>,
) -> Result<Response, AppError> {
    match state
        .turing
        .start(&form.artifact, form.review, form.voting)
        .await
    {
        Ok(()) => Ok(Redirect::to("/turing").into_response()),
        Err(error) => Ok((
            StatusCode::BAD_REQUEST,
            dashboard(&state, form, ChatPanel::current(true).await, Some(error)).await?,
        )
            .into_response()),
    }
}

async fn cancel(State(state): State<AppState>, Path(id): Path<Uuid>) -> Redirect {
    state.turing.cancel(id).await;
    Redirect::to("/turing")
}

async fn screen(State(state): State<AppState>) -> Result<Html<String>, AppError> {
    let content = render(StagePanel {
        stage: state.turing.stage().await,
        host: false,
    })?;
    Ok(Html(layout(
        "Turing Test",
        &format!(r#"<main class="turing-screen">{content}</main>"#),
    )))
}

#[derive(Deserialize)]
struct StageQuery {
    #[serde(default)]
    host: bool,
}
async fn stage(
    State(state): State<AppState>,
    Query(q): Query<StageQuery>,
) -> Result<Html<String>, AppError> {
    Ok(Html(render(StagePanel {
        stage: state.turing.stage().await,
        host: q.host,
    })?))
}

async fn media(State(state): State<AppState>, Path(id): Path<Uuid>) -> Result<Response, AppError> {
    let (mime, bytes) = state
        .turing
        .media(id)
        .await
        .ok_or_else(|| AppError::NotFound("Sample not found".into()))?;
    Ok((
        [
            ("content-type", mime),
            (
                "content-security-policy",
                "sandbox; default-src 'none'; style-src 'unsafe-inline'",
            ),
            ("x-content-type-options", "nosniff"),
        ],
        bytes,
    )
        .into_response())
}

#[derive(Deserialize)]
struct ScoreQuery {
    order: Option<String>,
}
async fn scores(
    State(state): State<AppState>,
    Query(q): Query<ScoreQuery>,
) -> Result<Html<String>, AppError> {
    let lowest = q.order.as_deref() == Some("lowest");
    let rows = state
        .turing
        .scores(lowest)
        .await
        .map_err(|e| AppError::Internal(format!("Cannot read viewer scores: {e}")))?;
    Ok(Html(layout(
        "Turing Test",
        &render(Standings {
            total: rows.iter().map(|s| s.total).sum(),
            correct: rows.iter().map(|s| s.correct).sum(),
            rows,
            lowest,
        })?,
    )))
}
