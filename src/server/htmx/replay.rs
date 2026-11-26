use super::{
    boards::render_human_state,
    layout::{html_escape, layout, render_status_badge},
};
use crate::server::state::ArenaMode;
use crate::{
    server::sessions::GameStatus,
    storage::{StoredRun, StoredTurn},
};

pub(in crate::server) fn render_replay_page(
    arena_mode: ArenaMode,
    game_id: &str,
    run: &StoredRun,
    events: &[StoredTurn],
    turn: Option<usize>,
) -> String {
    let selected = turn
        .unwrap_or_else(|| events.len().saturating_sub(1))
        .min(events.len().saturating_sub(1));

    let mut content = format!(
        r#"<main class="stack">
            <a class="back-link" href="/games/{}">← Back to match</a>
            <section class="panel">
                <div class="panel-heading"><div><p class="eyebrow">replay</p><h2>{}</h2><div class="session-id">{}</div></div>{}</div>"#,
        html_escape(game_id),
        html_escape(&run.game_type),
        html_escape(game_id),
        render_status_badge(&GameStatus::from_storage(&run.status)),
    );

    if events.is_empty() {
        content.push_str("<div class=\"empty\">This run has no recorded turns.</div>");
    } else {
        let event = &events[selected];
        content.push_str(&format!(
            "<p class=\"lede\">Turn {} of {} · {} · <code>{}</code></p>",
            selected + 1,
            events.len(),
            html_escape(&event.turn.player),
            html_escape(&event.turn.move_made.to_string())
        ));
        content.push_str(&render_human_state(
            &run.game_type,
            &run.agents,
            &event.turn.state_after,
        ));
        content.push_str("<div class=\"actions\">");
        if selected > 0 {
            content.push_str(&format!(
                "<a class=\"button small\" href=\"/games/{}/replay?turn={}\">← Previous</a>",
                html_escape(game_id),
                selected - 1
            ));
        }
        if selected + 1 < events.len() {
            content.push_str(&format!(
                "<a class=\"button small\" href=\"/games/{}/replay?turn={}\">Next →</a>",
                html_escape(game_id),
                selected + 1
            ));
        }
        content.push_str("</div>");
    }

    content.push_str("</section></main>");
    layout(arena_mode, "AI Arena Replay", &content)
}
