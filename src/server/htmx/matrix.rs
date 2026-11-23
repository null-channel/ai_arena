use super::layout::{html_escape, render_status_badge};
use crate::server::{
    jobs::{MatrixJobResponse, MatrixStatus},
    sessions::GameStatus,
};

pub(in crate::server) fn render_matrix_started(
    job: &MatrixJobResponse,
    matrix_fragment: &str,
) -> String {
    format!(
        r#"<div id="matrix-form"><div class="notice success"><strong>Matrix launched.</strong><span>{} runs with a concurrency limit of {}.</span></div></div><div id="matrix-list" hx-swap-oob="true">{}</div>"#,
        job.total, job.concurrency_limit, matrix_fragment,
    )
}

pub(in crate::server) fn render_matrix_fragment(jobs: &[MatrixJobResponse]) -> String {
    let mut html = String::from("<div id=\"matrix-list\">");
    if jobs.is_empty() {
        html.push_str("<div class=\"empty\">No matrix jobs yet.</div>");
    }
    for job in jobs {
        html.push_str(&format!(
            "<article class=\"session-card\"><div class=\"session-top\"><div><h3>{}</h3><div class=\"session-id\">{}</div></div><span class=\"badge {}\">{:?}</span></div><p>{}/{} complete · {} running · {} queued · {} failed · concurrency {}</p><div class=\"matrix-runs\">",
            html_escape(&job.game_type),
            html_escape(&job.matrix_id),
            match job.status { MatrixStatus::Queued => "pending", MatrixStatus::Running => "running", MatrixStatus::Completed => "completed" },
            job.status,
            job.completed + job.failed,
            job.total,
            job.running,
            job.queued,
            job.failed,
            job.concurrency_limit,
        ));
        for run in &job.runs {
            let replay = if matches!(run.status, GameStatus::Completed) {
                format!(
                    "<a href=\"/games/{}/replay\">Replay</a>",
                    html_escape(&run.run_id)
                )
            } else {
                String::new()
            };
            html.push_str(&format!(
                "<div><code>{}</code> {} <a href=\"/games/{}\">Observe</a> {}</div>",
                html_escape(&run.run_id),
                render_status_badge(&run.status),
                html_escape(&run.run_id),
                replay,
            ));
        }
        html.push_str("</div></article>");
    }
    html.push_str("</div>");
    html
}
