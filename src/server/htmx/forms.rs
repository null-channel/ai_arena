use super::layout::html_escape;
use crate::{
    agent_config::AgentKind,
    server::config::{AgentFormDefaults, FormDefaults, RunMode},
};

const GAMES: &[(&str, &str, &str)] = &[
    ("TicTacToe", "Tic-tac-toe", "×○"),
    ("ConnectFour", "Connect Four", "⠿"),
    ("RockPaperScissors", "Rock paper scissors", "✂"),
    ("Chess", "Chess", "♞"),
    ("Checkers", "Checkers", "⛀"),
    ("Battleship", "Battleship", "⚓"),
];

pub(super) fn game_label(game: &str) -> &str {
    GAMES
        .iter()
        .find(|(value, _, _)| *value == game)
        .map_or(game, |(_, label, _)| label)
}

pub(super) fn game_filters(path: &str, game: &str) -> String {
    let mut filters = String::from(r#"<nav class="filters" aria-label="Filter by game">"#);
    for &(value, label, _) in GAMES {
        let active = if value == game {
            " aria-current=\"page\""
        } else {
            ""
        };
        filters.push_str(&format!(
            r#"<a href="{path}?game={value}"{active}>{label}</a>"#
        ));
    }
    filters.push_str("</nav>");
    filters
}

const PROVIDERS: &[(AgentKind, &str, &[&str])] = &[
    (
        AgentKind::OpenAI,
        "OpenAI",
        &["gpt-5.4", "gpt-4.1", "gpt-4o-mini", "gpt-5-mini"],
    ),
    (
        AgentKind::Anthropic,
        "Anthropic",
        &["claude-sonnet-4-6", "claude-opus-4-7"],
    ),
    (
        AgentKind::Gemini,
        "Gemini",
        &["gemini-3.6-flash", "gemini-2.5-flash"],
    ),
    (AgentKind::Ollama, "Ollama", &["llama3", "qwen3.6:latest"]),
];

pub(in crate::server) fn render_start_form(
    profiles: &[String],
    defaults: &FormDefaults,
    error: Option<&str>,
) -> String {
    render_form(profiles, defaults, RunMode::Single, None, error)
}

pub(in crate::server) fn render_form(
    profiles: &[String],
    defaults: &FormDefaults,
    mode: RunMode,
    counts: Option<(u32, usize)>,
    error: Option<&str>,
) -> String {
    let (action, target, launch) = match mode {
        RunMode::Single => ("/games/start", "start-form", "Start match"),
        RunMode::Multi => ("/multi", "matrix-form", "Start multi"),
        RunMode::Matrix => ("/matrix", "matrix-form", "Start matrix"),
    };
    let mut html = format!(
        r##"<form method="post" action="{action}" hx-post="{action}" hx-target="#{target}" hx-swap="innerHTML" hx-disabled-elt="find button[type=submit]">"##
    );
    if let Some(error) = error {
        html.push_str(&format!(r#"<p class="notice failure" role="alert"><strong>Could not start match.</strong> {}</p>"#, html_escape(error)));
    }
    let results_path = match mode {
        RunMode::Single => "/history/summary",
        RunMode::Multi => "/multi/results",
        RunMode::Matrix => "/matrix/results",
    };
    html.push_str(&format!(r##"<fieldset><legend>Game type</legend><div class="game-options" hx-get="{results_path}" hx-trigger="change, every 10s" hx-target="#recent-results" hx-include="find input:checked" hx-sync="this:replace">"##));
    for &(value, label, icon) in GAMES {
        let checked = if defaults.game_type == value {
            "checked"
        } else {
            ""
        };
        html.push_str(&format!(r#"<label class="radio-card"><input type="radio" name="game_type" value="{value}" {checked}><span><i aria-hidden="true">{icon}</i>{label}</span></label>"#));
    }
    html.push_str("</div></fieldset>");
    let roster = mode == RunMode::Matrix;
    if mode != RunMode::Single {
        let (repetitions, concurrency) = counts.unwrap_or(if roster { (3, 4) } else { (10, 4) });
        let repeat_label = if roster {
            "Matches per pair"
        } else {
            "Matches"
        };
        html.push_str(&format!(r#"<fieldset><legend>{} settings</legend><div class="form-grid">
            <label>{repeat_label}<input name="repetitions" type="number" min="1" max="1000" value="{repetitions}" required></label>
            <label>Concurrent matches<input name="concurrency_limit" type="number" min="1" max="32" value="{concurrency}" required></label>
        </div></fieldset>"#, mode.label()));
    }
    let legend = if roster { "Agent roster" } else { "Agents" };
    html.push_str(&format!(
        r#"<fieldset><legend>{legend}</legend><div class="form-grid" id="agent-list">"#
    ));
    for (prefix, player, agent) in [
        (
            if roster { "slot1" } else { "agent_one" },
            1,
            &defaults.agent_one,
        ),
        (
            if roster { "slot2" } else { "agent_two" },
            2,
            &defaults.agent_two,
        ),
    ] {
        html.push_str(&render_agent(prefix, player, agent, profiles, roster));
    }
    html.push_str("</div>");
    if roster {
        let mut blank = defaults.agent_one.clone();
        blank.model.clear();
        html.push_str(&format!(r#"<template id="agent-template">{}</template>
            <button type="button" class="roster-action" data-add-agent>+ Add agent</button>
            <output class="schedule-count" data-match-count data-limit="{}" aria-live="polite">1 pair · 3 matches total</output>
            <div id="matrix-feedback" role="alert"></div>"#,
            render_agent("newslot", 3, &blank, profiles, true), crate::server::jobs::MAX_JOB_RUNS));
    }
    html.push_str(&format!(r#"</fieldset><button class="launch" type="submit"><span aria-hidden="true">▶</span> {launch}</button></form>"#));
    html
}

pub(in crate::server) fn render_agent(
    prefix: &str,
    player: u8,
    agent: &AgentFormDefaults,
    profiles: &[String],
    roster: bool,
) -> String {
    let mut options = String::new();
    let mut model_list = String::new();
    for &(kind, label, models) in PROVIDERS {
        let selected = if kind == agent.kind {
            for model in models {
                model_list.push_str(&format!(r#"<option value="{model}"></option>"#));
            }
            " selected"
        } else {
            ""
        };
        let models = models.join(" ");
        let temperatures = kind.temperature_models().join(" ");
        options.push_str(&format!(r#"<option value="{label}" data-models="{models}" data-temperature="{temperatures}"{selected}>{label}</option>"#));
    }
    let model = html_escape(&agent.model);
    let prompt = html_escape(agent.prompt_file.as_deref().unwrap_or(""));
    let endpoint = html_escape(
        agent
            .api_endpoint
            .as_deref()
            .unwrap_or("http://localhost:11434"),
    );
    let profile = html_escape(agent.secret_profile.as_deref().unwrap_or(""));
    let profiles = profiles
        .iter()
        .map(|name| format!(r#"<option value="{}"></option>"#, html_escape(name)))
        .collect::<String>();
    let remove = if roster {
        r#"<button type="button" class="roster-action" data-remove-agent disabled>Remove</button>"#
    } else {
        ""
    };
    let temperature = agent.temp;
    let (temp_hidden, temp_disabled) = field_state(agent.kind.supports_temperature(&agent.model));
    let (endpoint_hidden, endpoint_disabled) = field_state(agent.kind == AgentKind::Ollama);
    let (profile_hidden, profile_disabled) = field_state(agent.kind != AgentKind::Ollama);
    format!(
        r#"<section class="agent-card" data-agent>
        <div class="session-top"><h3><span class="player-token p{player}" data-seat aria-hidden="true">{player}</span> Agent <span data-seat>{player}</span></h3>{remove}</div>
        <label>Provider<select name="{prefix}_kind" data-provider>{options}</select></label>
        <label>Model<input name="{prefix}_model" data-model-input list="{prefix}-models" value="{model}" required autocomplete="off" spellcheck="false"></label>
        <datalist id="{prefix}-models" data-model-list>{model_list}</datalist>
        <label data-field="temperature"{temp_hidden}>Temperature<input name="{prefix}_temp" type="number" min="0" max="2" step="any" value="{temperature}" required{temp_disabled}></label>
        <label data-field="endpoint"{endpoint_hidden}>Ollama endpoint<input type="url" name="{prefix}_api_endpoint" value="{endpoint}" required{endpoint_disabled}></label>
        <details><summary>Advanced settings</summary>
            <label data-field="profile"{profile_hidden}>Secret profile<input name="{prefix}_secret_profile" list="{prefix}-profiles" value="{profile}" placeholder="Environment default"{profile_disabled}></label>
            <datalist id="{prefix}-profiles">{profiles}</datalist>
            <label>Prompt file<input name="{prefix}_prompt_file" value="{prompt}" placeholder="/path/to/instructions.txt"></label>
        </details>
    </section>"#
    )
}

fn field_state(available: bool) -> (&'static str, &'static str) {
    if available {
        ("", "")
    } else {
        (" hidden", " disabled")
    }
}
