use super::{forms::render_form, layout::layout};
use crate::server::config::{FormDefaults, RunMode};
use crate::server::state::ArenaMode;

pub(in crate::server) fn render_index_page(
    arena_mode: ArenaMode,
    profiles: &[String],
    defaults: &FormDefaults,
    activity: &str,
    mode: RunMode,
    recent: &str,
) -> String {
    let form_id = if mode == RunMode::Single {
        "start-form"
    } else {
        "matrix-form"
    };
    let tabs = [RunMode::Single, RunMode::Multi, RunMode::Matrix]
        .into_iter()
        .map(|tab| {
            let current = if tab == mode {
                " aria-current=\"page\""
            } else {
                ""
            };
            format!(r#"<a href="{}"{current}>{}</a>"#, tab.path(), tab.label())
        })
        .collect::<String>();
    let (activity_heading, activity_path) = match mode {
        RunMode::Single => (
            r#"<i class="live-dot" aria-hidden="true"></i>Live matches"#,
            "/games",
        ),
        RunMode::Multi => ("Batches", "/multi/activity"),
        RunMode::Matrix => ("Batches", "/matrix/activity"),
    };
    let content = format!(
        r#"
        <main>
            <div class="grid">
                <section class="panel setup">
                    <div class="panel-heading"><h1>Match setup</h1>
                        <nav class="switch" aria-label="Match mode">{tabs}</nav>
                    </div>
                    <div id="{form_id}">{form}</div>
                </section>
                <aside class="stack">
                    <section class="panel"><h2>{activity_heading}</h2>
                        <div class="activity" hx-get="{activity_path}" hx-trigger="every 3s">{activity}</div>
                    </section>
                    <section class="panel"><h2>Recent results</h2><div id="recent-results" aria-live="polite">{recent}</div></section>
                </aside>
            </div>
        </main>"#,
        form = render_form(profiles, defaults, mode, None, None),
    );
    layout(arena_mode, "Console", "/", &content)
}
