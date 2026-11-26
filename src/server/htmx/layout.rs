use crate::server::{sessions::GameStatus, state::ArenaMode};

struct Section {
    name: &'static str,
    views: &'static [(&'static str, &'static str)],
}

const SECTIONS: &[Section] = &[
    Section {
        name: "AI Games",
        views: &[
            ("/", "Console"),
            ("/history", "Run history"),
            ("/rankings", "Leaderboard"),
        ],
    },
    Section {
        name: "Tool Calling",
        views: &[
            ("/toolbench", "Console"),
            ("/toolbench/history", "Run history"),
        ],
    },
    Section {
        name: "KubeBench",
        views: &[
            ("/kubebench", "Console"),
            ("/kubebench/history", "Run history"),
        ],
    },
    Section {
        name: "Turing Test",
        views: &[
            ("/turing", "Console"),
            ("/turing/scores", "Viewer standings"),
        ],
    },
];

pub(in crate::server) fn layout(
    mode: ArenaMode,
    title: &str,
    view_path: &str,
    content: &str,
) -> String {
    let mode_key = mode.key();
    let checked = if mode == ArenaMode::Real {
        " checked"
    } else {
        ""
    };
    let section = SECTIONS
        .iter()
        .skip(1)
        .find(|section| view_path.split('/').nth(1) == section.views[0].0.split('/').nth(1))
        .unwrap_or(&SECTIONS[0]);
    let section_name = section.name;
    let mut links = String::new();
    for &(href, label) in section.views {
        let active = if href == view_path {
            r#" aria-current="page""#
        } else {
            ""
        };
        links.push_str(&format!(r#"<a href="{href}"{active}>{label}</a>"#));
    }
    let view_nav =
        format!(r#"<nav class="section-views" aria-label="{section_name} views">{links}</nav>"#);
    let mut sections = String::new();
    for item in SECTIONS {
        let name = item.name;
        let path = item.views[0].0;
        let active = if name == section_name {
            r#" aria-current="true""#
        } else {
            ""
        };
        sections.push_str(&format!(r#"<a href="{path}"{active}>{name}</a>"#));
    }
    format!(
        r#"<!doctype html><html lang="en" data-theme="dark" data-mode="{mode_key}"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
        <title>{} · AI Arena</title><meta name="color-scheme" content="dark light"><script src="/assets/arena.js"></script><link rel="stylesheet" href="/assets/arena.css">
        <script defer src="/assets/htmx.js"></script></head>
        <body><header class="topbar"><div class="topinner">
        <div class="brand-group">
            <details class="section-menu"><summary aria-label="Navigation menu"><span class="hamburger" aria-hidden="true"></span></summary>
                <div class="section-panel"><nav aria-label="Arena sections">{sections}</nav>{view_nav}
                    <label class="theme-control" for="arena-mode">Save results
                        <input id="arena-mode" type="checkbox"{checked} aria-describedby="mode-status">
                    </label>
                    <div id="mode-status"><span data-mode="test">Testing</span><span data-mode="real">Recording</span></div>
                    <span id="mode-error" role="alert" hidden>Could not update Save results.</span>
                    <label class="theme-control" for="theme">Theme
                        <select id="theme"><option value="dark" selected>Dark</option><option value="light">Light</option><option value="system">System</option></select>
                    </label>
                </div>
            </details>
            <a class="brand" href="/"><i aria-hidden="true"></i>AI Arena</a>
            <span class="section-name">{section_name}</span>
        </div>{view_nav}</div></header><div class="shell">{content}</div></body></html>"#,
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
