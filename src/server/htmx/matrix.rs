use super::layout::{html_escape, layout, render_status_badge};
use crate::server::state::ArenaMode;
use crate::{
    server::{config::RunMode, sessions::GameStatus},
    storage::{BatchSession, BatchSummary},
};
use std::fmt::Write;

pub(in crate::server) fn render_batch_activity(
    mode: RunMode,
    tests: &[BatchSummary],
    batches: &[BatchSummary],
) -> String {
    if batches.is_empty() && tests.is_empty() {
        return "<div class=\"empty\">No batches yet.</div>".into();
    }
    let mut html = String::new();
    for (batch, label) in tests
        .iter()
        .map(|b| (b, ArenaMode::Test.run_label()))
        .chain(batches.iter().map(|b| (b, ArenaMode::Real.run_label())))
    {
        write!(html, r#"<a class="recent-run" href="{}/{}"><strong>{}</strong><small>{label} · {} of {} sessions finished · {}</small></a>"#,
            mode.path(), html_escape(&batch.id), html_escape(&batch.game_type), batch.finished,
            batch.total, html_escape(&batch.id.chars().take(8).collect::<String>())).unwrap();
    }
    html
}

pub(in crate::server) fn render_batch_page(
    arena_mode: ArenaMode,
    mode: RunMode,
    id: &str,
    game: &str,
    sessions: &[BatchSession],
    job_mode: Option<ArenaMode>,
) -> String {
    let id = html_escape(id);
    let path = mode.path();
    let label = mode.label();
    let tracked = job_mode.is_some();
    let recording = job_mode.unwrap_or(ArenaMode::Real).run_label();
    let finished = sessions
        .iter()
        .filter(|s| matches!(s.status.as_str(), "completed" | "error"))
        .count();
    let poll = if tracked && finished < sessions.len() {
        format!(
            r##" hx-get="{path}/{id}" hx-trigger="every 2s" hx-select="#batch-sessions" hx-swap="outerHTML""##
        )
    } else {
        String::new()
    };
    let mut rows = String::new();
    for (index, session) in sessions.iter().enumerate() {
        let models = session
            .agents
            .iter()
            .map(|a| html_escape(&a.model))
            .collect::<Vec<_>>()
            .join(" vs ");
        let status = GameStatus::from_storage(&session.status);
        let run = html_escape(&session.id);
        let replay = if matches!(status, GameStatus::Completed) {
            format!(r#"<a href="/games/{run}/replay">Replay</a>"#)
        } else {
            String::new()
        };
        let badge = if !tracked && matches!(status, GameStatus::Pending | GameStatus::Running) {
            r#"<span class="badge error">Interrupted</span>"#.into()
        } else {
            render_status_badge(&status)
        };
        write!(rows, r#"<tr><td><a href="/games/{run}">Session {}</a></td><td>{models}</td><td>{}</td><td>{badge}</td><td>{replay}</td></tr>"#,
            index + 1, session.repetition.map(|r| r.to_string()).unwrap_or_else(|| "—".into())).unwrap();
    }
    let content = format!(
        r#"<main class="stack"><a class="back-link" href="{path}">← {label} setup</a>
        <div class="page-heading"><h1>{label} · {}</h1><p class="session-id">{recording} · {id}</p></div>
        <section class="panel" id="batch-sessions"{poll}><div class="panel-heading"><h2>Sessions</h2><span>{finished} of {} finished</span></div>
        <div class="table-scroll"><table><thead><tr><th>Session</th><th>Matchup</th><th>Round</th><th>Status</th><th>Replay</th></tr></thead><tbody>{rows}</tbody></table></div></section></main>"#,
        html_escape(game),
        sessions.len()
    );
    layout(arena_mode, label, "/", &content)
}
