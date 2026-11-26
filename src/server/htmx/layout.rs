use crate::server::sessions::GameStatus;

pub(in crate::server) fn layout(title: &str, content: &str) -> String {
    let in_games = title != "Tool Calling";
    let game_nav = if in_games {
        let view = match title {
            "Run history" | "Leaderboard" => title,
            _ => "Console",
        };
        let mut links = String::new();
        for (href, label) in [
            ("/", "Console"),
            ("/history", "Run history"),
            ("/rankings", "Leaderboard"),
        ] {
            let active = if label == view {
                r#" aria-current="page""#
            } else {
                ""
            };
            links.push_str(&format!(r#"<a href="{href}"{active}>{label}</a>"#));
        }
        format!(r#"<nav class="game-views" aria-label="Game views">{links}</nav>"#)
    } else {
        String::new()
    };
    let current_section = r#" aria-current="true""#;
    let (section_name, games_active, tools_active) = if in_games {
        ("AI Games", current_section, "")
    } else {
        ("Tool Calling", "", current_section)
    };
    format!(
        r#"<!doctype html><html lang="en" data-theme="dark"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
        <title>{} · AI Arena</title><meta name="color-scheme" content="dark light"><script src="/assets/arena.js"></script><link rel="stylesheet" href="/assets/arena.css">
        <script defer src="/assets/htmx.js"></script></head>
        <body><header class="topbar"><div class="topinner">
        <div class="brand-group">
            <details class="section-menu"><summary aria-label="Navigation menu"><span class="hamburger" aria-hidden="true"></span></summary>
                <div class="section-panel"><nav aria-label="Arena sections"><a href="/"{games_active}>AI Games</a><a href="/toolbench"{tools_active}>Tool Calling</a></nav>{game_nav}
                    <label class="theme-control" for="theme">Theme
                        <select id="theme"><option value="dark" selected>Dark</option><option value="light">Light</option><option value="system">System</option></select>
                    </label>
                </div>
            </details>
            <a class="brand" href="/"><i aria-hidden="true"></i>AI Arena</a>
            <span class="section-name">{section_name}</span>
        </div>{game_nav}</div></header><div class="shell">{content}</div></body></html>"#,
        html_escape(title)
    )
}

pub(super) fn render_status_badge(status: &GameStatus) -> String {
    let status_text = match status {
        GameStatus::Pending => "pending",
        GameStatus::Running => "running",
        GameStatus::Completed => "completed",
        GameStatus::Error => "error",
    };
    format!(
        "<span class=\"badge {}\">{}</span>",
        status_text, status_text
    )
}

pub(super) fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
