use super::layout::html_escape;
use crate::{
    agent_config::AgentKind,
    server::config::{AgentFormDefaults, FormDefaults},
};

const GAME_OPTIONS: &[(&str, &str, &str)] = &[
    ("TicTacToe", "Tic-Tac-Toe", "Fast tactical baseline"),
    ("ConnectFour", "Connect Four", "Spatial planning and traps"),
    (
        "RockPaperScissors",
        "Rock Paper Scissors",
        "Short-form strategy loop",
    ),
    ("Chess", "Chess", "Long-horizon reasoning"),
    ("Checkers", "Checkers", "Forced captures and promotion"),
    (
        "Battleship",
        "Battleship",
        "Hidden fleets and targeted shots",
    ),
];

const AGENT_OPTIONS: &[(&str, &str)] = &[
    ("Ollama", "Ollama"),
    ("OpenAI", "OpenAI"),
    ("Anthropic", "Anthropic"),
    ("Gemini", "Gemini"),
];

pub(in crate::server) fn render_start_form(
    secret_profiles: &[String],
    defaults: &FormDefaults,
    error: Option<&str>,
) -> String {
    let mut html = String::new();
    html.push_str(
        "<form hx-post=\"/games/start\" hx-target=\"#start-form\" hx-swap=\"outerHTML\" hx-trigger=\"submit\" hx-on:change=\"htmx.ajax('POST', '/games/defaults', {source: this, swap: 'none'})\">",
    );

    if let Some(error) = error {
        html.push_str(&format!(
            "<div class=\"notice failure\"><strong>Could not start match.</strong><span>{}</span></div>",
            html_escape(error)
        ));
    }

    html.push_str("<div class=\"game-options\">");
    for (value, label, description) in GAME_OPTIONS {
        html.push_str(&format!(
            r#"<label class="radio-card">
                <input type="radio" name="game_type" value="{}" {}>
                <span><strong>{}</strong><small>{}</small></span>
            </label>"#,
            value,
            if *value == defaults.game_type {
                "checked"
            } else {
                ""
            },
            label,
            description
        ));
    }
    html.push_str("</div>");

    html.push_str("<div class=\"form-grid\">");
    html.push_str(&render_agent_form(
        1,
        "agent_one",
        &defaults.agent_one,
        secret_profiles,
    ));
    html.push_str(&render_agent_form(
        2,
        "agent_two",
        &defaults.agent_two,
        secret_profiles,
    ));
    html.push_str("</div>");
    html.push_str(
        r#"<div class="actions">
            <button type="submit">Start Battle</button>
            <span class="metric-label">Runs server-side; page updates with HTMX fragments.</span>
        </div>"#,
    );
    html.push_str("</form>");
    html
}

pub(in crate::server) fn render_matrix_form(
    defaults: &FormDefaults,
    error: Option<&str>,
) -> String {
    let mut html = String::from(
        "<form hx-post=\"/matrix/start\" hx-target=\"#matrix-form\" hx-swap=\"outerHTML\">",
    );
    if let Some(error) = error {
        html.push_str(&format!(
            "<div class=\"notice failure\"><strong>Could not start matrix.</strong><span>{}</span></div>",
            html_escape(error)
        ));
    }
    html.push_str("<div class=\"form-grid\">");
    html.push_str("<div class=\"agent-card\"><h3>Matrix Settings</h3>");
    html.push_str("<div class=\"field\"><label>Game</label><select name=\"game_type\">");
    for (value, label, _) in GAME_OPTIONS {
        html.push_str(&format!(
            "<option value=\"{}\"{}>{}</option>",
            value,
            if *value == defaults.game_type {
                " selected"
            } else {
                ""
            },
            label
        ));
    }
    html.push_str("</select></div>");
    html.push_str("<div class=\"field\"><label>Repetitions</label><input name=\"repetitions\" type=\"number\" min=\"1\" max=\"1000\" value=\"10\" required></div>");
    html.push_str("<div class=\"field\"><label>Concurrency</label><input name=\"concurrency_limit\" type=\"number\" min=\"1\" max=\"32\" value=\"4\" required></div></div>");
    html.push_str(&render_matrix_agent_form(
        "agent_one",
        "Player 1",
        &defaults.agent_one,
    ));
    html.push_str(&render_matrix_agent_form(
        "agent_two",
        "Player 2",
        &defaults.agent_two,
    ));
    html.push_str("</div><div class=\"actions\"><button type=\"submit\">Start Matrix</button><span class=\"metric-label\">Child runs are bounded and persisted individually.</span></div></form>");
    html
}

fn render_matrix_agent_form(prefix: &str, title: &str, defaults: &AgentFormDefaults) -> String {
    format!(
        r#"<div class="agent-card"><h3>{}</h3>
            <div class="field"><label>Provider</label><select name="{}_kind">{}</select></div>
            <div class="field"><label>Model</label><input name="{}_model" value="{}" required></div>
            <div class="field"><label>Temperature</label><input name="{}_temp" type="number" min="0" max="2" step="0.1" value="{}" required></div>
        </div>"#,
        html_escape(title),
        prefix,
        render_agent_options(kind_name(defaults.kind)),
        prefix,
        html_escape(&defaults.model),
        prefix,
        defaults.temp,
    )
}

fn render_agent_form(
    player: u8,
    prefix: &str,
    defaults: &AgentFormDefaults,
    secret_profiles: &[String],
) -> String {
    format!(
        r#"
        <div class="agent-card">
            <h3>Player {}</h3>
            <div class="field">
                <label for="{prefix}_kind">Provider</label>
                <select id="{prefix}_kind" name="{prefix}_kind">{}</select>
            </div>
            <div class="field">
                <label for="{prefix}_model">Model</label>
                <input id="{prefix}_model" name="{prefix}_model" value="{}" required>
            </div>
            <div class="field">
                <label for="{prefix}_temp">Temperature</label>
                <input id="{prefix}_temp" name="{prefix}_temp" type="number" min="0" max="2" step="0.1" value="{}" required>
            </div>
            <div class="field">
                <label for="{prefix}_seed">Seed</label>
                <input id="{prefix}_seed" name="{prefix}_seed" type="number" min="0" value="{}">
            </div>
            <div class="field">
                <label for="{prefix}_secret_profile">Secret Profile</label>
                {}
            </div>
            <div class="field">
                <label for="{prefix}_api_endpoint">API Endpoint</label>
                <input id="{prefix}_api_endpoint" name="{prefix}_api_endpoint" value="{}">
            </div>
            <div class="field">
                <label for="{prefix}_prompt_file">Prompt File</label>
                <input id="{prefix}_prompt_file" name="{prefix}_prompt_file" value="{}" placeholder="/path/to/instructions.txt">
            </div>
        </div>
        "#,
        player,
        render_agent_options(kind_name(defaults.kind)),
        html_escape(&defaults.model),
        defaults.temp,
        defaults
            .seed
            .map(|seed| seed.to_string())
            .unwrap_or_default(),
        render_secret_profile_input(prefix, secret_profiles, defaults.secret_profile.as_deref()),
        html_escape(defaults.api_endpoint.as_deref().unwrap_or("")),
        html_escape(defaults.prompt_file.as_deref().unwrap_or("")),
    )
}

fn render_agent_options(selected: &str) -> String {
    AGENT_OPTIONS
        .iter()
        .map(|(value, label)| {
            format!(
                "<option value=\"{}\"{}>{}</option>",
                value,
                if *value == selected { " selected" } else { "" },
                label
            )
        })
        .collect::<Vec<_>>()
        .join("")
}

pub(super) fn kind_name(kind: AgentKind) -> &'static str {
    match kind {
        AgentKind::OpenAI => "OpenAI",
        AgentKind::Anthropic => "Anthropic",
        AgentKind::Gemini => "Gemini",
        AgentKind::Ollama => "Ollama",
    }
}

fn render_secret_profile_input(
    prefix: &str,
    secret_profiles: &[String],
    selected: Option<&str>,
) -> String {
    if secret_profiles.is_empty() {
        return format!(
            "<input id=\"{prefix}_secret_profile\" name=\"{prefix}_secret_profile\" value=\"{}\" placeholder=\"optional profile name\">",
            html_escape(selected.unwrap_or(""))
        );
    }

    let mut options = String::new();
    if let Some(selected) = selected.filter(|selected| {
        !selected.is_empty() && !secret_profiles.iter().any(|profile| profile == selected)
    }) {
        options.push_str(&format!(
            "<option value=\"{}\" selected>{}</option>",
            html_escape(selected),
            html_escape(selected)
        ));
    }

    options.push_str(
        &secret_profiles
            .iter()
            .map(|profile| {
                format!(
                    "<option value=\"{}\"{}>{}</option>",
                    html_escape(profile),
                    if Some(profile.as_str()) == selected {
                        " selected"
                    } else {
                        ""
                    },
                    html_escape(profile)
                )
            })
            .collect::<Vec<_>>()
            .join(""),
    );

    format!(
        "<select id=\"{prefix}_secret_profile\" name=\"{prefix}_secret_profile\"><option value=\"\">Environment/default</option>{options}</select>"
    )
}
