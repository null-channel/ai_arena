use super::{
    forms::{game_filters, game_label},
    layout::{html_escape, layout},
};
use crate::server::config::RunMode;
use crate::server::state::ArenaMode;
use crate::storage::{BatchResult, GameHistory, HISTORY_PAGE_SIZE, RunMetrics};

pub(in crate::server) fn render_recent_results(game: &str, runs: &[RunMetrics]) -> String {
    let label = game_label(game);
    let mut html = format!(r#"<p class="recent-game">{label}</p>"#);
    if runs.is_empty() {
        html.push_str(r#"<p class="empty">No finished runs for this game yet.</p>"#);
    }
    for run in runs {
        let details = [
            run.turns.map(|n| format!("{n} turns")),
            run.duration_ms.map(|ms| duration(ms as f64)),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" · ");
        let details = if details.is_empty() {
            details
        } else {
            format!(" · {details}")
        };
        html.push_str(&format!(r#"<a class="recent-run" href="/games/{}"><strong>{}</strong><small>{} UTC{details}</small></a>"#,
            html_escape(&run.id), html_escape(&run.outcome), html_escape(&run.listed_at)));
    }
    html.push_str(&format!(r#"<a class="foot-link" href="/history?game={game}">View game history <span aria-hidden="true">→</span></a>"#));
    html
}

pub(in crate::server) fn render_recent_batches(
    game: &str,
    mode: RunMode,
    batches: &[BatchResult],
) -> String {
    let mut html = format!(r#"<p class="recent-game">{}</p>"#, game_label(game));
    if batches.is_empty() {
        html.push_str(r#"<p class="empty">No finished batches for this game yet.</p>"#);
    }
    for batch in batches {
        let matches = if batch.total == 1 { "match" } else { "matches" };
        let draws = if batch.draws == 1 { "draw" } else { "draws" };
        html.push_str(&format!(r#"<a class="recent-run" href="{}/{}"><strong>{} {matches}</strong><small>{} UTC · {} {draws} · {} failed</small></a>"#,
            mode.path(), html_escape(&batch.id), batch.total, html_escape(&batch.finished_at), batch.draws, batch.failed));
    }
    html
}

pub(in crate::server) fn render_history(
    arena_mode: ArenaMode,
    game: &str,
    page: u32,
    history: GameHistory,
) -> String {
    let filters = game_filters("/history", game);
    let heading = game_label(game);
    let mut content = format!(
        r#"<main><header class="page-heading"><h1>Run history</h1><p>Explore results and performance by game type.</p></header>
        {filters}
        <section class="panel"><div class="panel-heading"><h2>{heading}</h2></div>
        <div class="result-grid">"#
    );
    let avg = history
        .avg_duration_ms
        .map(duration)
        .unwrap_or_else(|| "—".into());
    for (name, value) in [
        ("Completed", history.completed.to_string()),
        ("Failed", history.failed.to_string()),
        ("Draws", history.draws.to_string()),
        ("Avg completed duration", avg),
    ] {
        content.push_str(&format!(
            "<div class=\"stat\"><span>{name}</span><strong>{value}</strong></div>"
        ));
    }
    content.push_str("</div><h3>Previous runs</h3>");
    if history.runs.is_empty() {
        content.push_str("<div class=\"empty\">No finished runs on this page.</div>");
    } else {
        content.push_str(r#"<div class="table-scroll"><table><thead><tr><th>Created (UTC)</th><th>Matchup</th><th>Outcome</th><th>Turns</th><th>Invalid moves</th><th>Duration</th><th>Details</th></tr></thead><tbody>"#);
        for run in history.runs {
            let matchup = run
                .agents
                .iter()
                .map(|a| html_escape(&a.model))
                .collect::<Vec<_>>()
                .join(" vs ");
            let id = html_escape(&run.id);
            content.push_str(&format!(r#"<tr><td>{}</td><td>{matchup}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td><a href="/games/{id}">Open run</a></td></tr>"#,
                html_escape(&run.listed_at), html_escape(&run.outcome), count(run.turns), count(run.invalid_moves),
                run.duration_ms.map(|ms| duration(ms as f64)).unwrap_or_else(|| "—".into())));
        }
        content.push_str("</tbody></table></div>");
    }
    content.push_str("<nav class=\"actions\" aria-label=\"History pages\">");
    if page > 1 {
        content.push_str(&format!(
            r#"<a href="/history?game={game}&amp;page={}">← Newer runs</a>"#,
            page - 1
        ));
    }
    if i64::from(page) * i64::from(HISTORY_PAGE_SIZE) < history.completed + history.failed {
        content.push_str(&format!(
            r#"<a href="/history?game={game}&amp;page={}">Older runs →</a>"#,
            u64::from(page) + 1
        ));
    }
    content.push_str("</nav></section></main>");
    layout(arena_mode, "Run history", &content)
}

fn count(value: Option<i64>) -> String {
    value.map(|n| n.to_string()).unwrap_or_else(|| "—".into())
}

fn duration(ms: f64) -> String {
    if ms < 1000.0 {
        format!("{ms:.0} ms")
    } else {
        format!("{:.1} s", ms / 1000.0)
    }
}
