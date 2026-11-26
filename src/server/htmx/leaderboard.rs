use super::{
    forms::{game_filters, game_label},
    layout::{html_escape, layout},
};
use crate::server::state::ArenaMode;
use crate::storage::LeaderboardEntry;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use std::fmt::Write;

pub(in crate::server) fn render_rankings_page(
    arena_mode: ArenaMode,
    game: &str,
    scores: &str,
) -> String {
    let filters = game_filters("/rankings", game);
    let label = game_label(game);
    layout(
        arena_mode,
        "Leaderboard",
        "/rankings",
        &format!(
            r#"<main>
        <header class="page-heading"><h1>Leaderboard</h1><p>Compare model performance for each game.</p></header>
        {filters}<section class="panel"><div class="panel-heading"><h2>{label}</h2></div>
        <div class="table-scroll" hx-get="/leaderboard?game={game}" hx-trigger="every 15s">{scores}</div>
        </section></main>"#
        ),
    )
}

pub(in crate::server) fn render_leaderboard_fragment(
    entries: Result<Vec<LeaderboardEntry>, String>,
) -> String {
    let entries = match entries {
        Ok(entries) => entries,
        Err(error) => {
            return format!(
                "<div id=\"leaderboard\" class=\"notice failure\">Could not load leaderboard: {}</div>",
                html_escape(&error)
            );
        }
    };
    let mut html = String::from("<div id=\"leaderboard\">");
    if entries.is_empty() {
        html.push_str("<div class=\"empty\">No ranked runs for this game yet.</div>");
    } else {
        html.push_str("<table><thead><tr><th>Model</th><th>W–L–D</th><th>Games</th><th>Invalid</th><th>Avg turn</th></tr></thead><tbody>");
        for entry in entries {
            let key = URL_SAFE_NO_PAD.encode(&entry.model);
            let model = html_escape(&entry.model);
            write!(html, r#"<tr><td><button type="button" class="model-toggle" id="model-{key}" aria-expanded="false" aria-controls="opponents-{key}">{model}</button></td>
                <td>{}–{}–{}</td><td>{}</td><td>{}</td><td>{:.0}ms</td></tr>
                <tr id="opponents-{key}" class="opponent-detail" hidden><td colspan="5">"#,
                entry.wins, entry.losses, entry.draws, entry.games, entry.invalid_moves, entry.average_turn_time_ms).unwrap();
            render_opponents(&mut html, &entry);
            html.push_str("</td></tr>");
        }
        html.push_str("</tbody></table>");
    }
    html.push_str("</div>");
    html
}

fn render_opponents(html: &mut String, entry: &LeaderboardEntry) {
    let mut paired = 0;
    if !entry.opponents.is_empty() {
        write!(html, r#"<table><caption>Head-to-head record for {}</caption><thead><tr><th>Opponent</th><th>Games</th><th>Wins</th><th>Losses</th><th>Draws</th><th>Unscored</th></tr></thead><tbody>"#, html_escape(&entry.model)).unwrap();
        for (opponent, record) in &entry.opponents {
            paired += record.games;
            let unscored = record.games - record.wins - record.losses - record.draws;
            let self_play = if opponent == &entry.model {
                " (self-play)"
            } else {
                ""
            };
            write!(html, "<tr><th scope=\"row\">{}{self_play}</th><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{unscored}</td></tr>",
                html_escape(opponent), record.games, record.wins, record.losses, record.draws).unwrap();
        }
        html.push_str("</tbody></table>");
        if entry.opponents.contains_key(&entry.model) {
            html.push_str("<p>Self-play counts one appearance for each seat.</p>");
        }
    }
    if paired < entry.games {
        write!(
            html,
            "<p>Appearances without a recorded opponent: {}.</p>",
            entry.games - paired
        )
        .unwrap();
    }
}
