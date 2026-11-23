use super::layout::html_escape;
use crate::storage::LeaderboardEntry;

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
        html.push_str("<div class=\"empty\">Complete a match to populate the leaderboard.</div>");
    } else {
        html.push_str("<table><thead><tr><th>Model</th><th>W–L–D</th><th>Games</th><th>Invalid</th><th>Avg turn</th></tr></thead><tbody>");
        for entry in entries {
            html.push_str(&format!(
                "<tr><td>{}</td><td>{}–{}–{}</td><td>{}</td><td>{}</td><td>{:.0}ms</td></tr>",
                html_escape(&entry.model),
                entry.wins,
                entry.losses,
                entry.draws,
                entry.games,
                entry.invalid_moves,
                entry.average_turn_time_ms,
            ));
        }
        html.push_str("</tbody></table>");
    }
    html.push_str("</div>");
    html
}
