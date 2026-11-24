use super::{
    boards::{render_human_state, render_player_legend},
    layout::{html_escape, layout, render_status_badge},
};
use crate::{
    agent_config::AIAgentConfig,
    games::{events::GameEvent, game::TestResult, stats::GameStats},
    server::{
        error::AppError,
        sessions::{GameStatus, GameSummary},
    },
};
use askama::Template;
use serde_json::Value;

#[derive(Template)]
#[template(path = "battle_log.html")]
struct BattleLog<'a> {
    game_id: &'a str,
    turns: Vec<&'a Value>,
    live: bool,
}

pub(in crate::server) fn render_game_page(
    fragment: &str,
    parent: Option<(String, String)>,
) -> String {
    let (href, label) = match parent {
        Some((mode, id)) if mode == "multi" => {
            (format!("/multi/{}", html_escape(&id)), "Multi sessions")
        }
        Some((mode, id)) if mode == "matrix" => {
            (format!("/matrix/{}", html_escape(&id)), "Matrix sessions")
        }
        _ => ("/".into(), "Console"),
    };
    let content = format!(
        r#"
        <main class="stack">
            <a class="back-link" href="{href}">← {label}</a>
            <section class="panel">{fragment}</section>
        </main>
        "#,
    );
    layout("AI Arena Game", &content)
}

pub(in crate::server) fn render_games_fragment(sessions: &[GameSummary]) -> String {
    let mut html = String::from(r#"<div id="session-list">"#);
    if sessions.is_empty() {
        html.push_str(r#"<div class="empty"><span class="idle-icon" aria-hidden="true">◌</span><strong>No active matches</strong><p>Matches appear here while they play.</p></div>"#);
    }
    for game in sessions {
        let matchup = game
            .agents
            .iter()
            .map(|a| html_escape(&a.model))
            .collect::<Vec<_>>()
            .join(" vs ");
        html.push_str(&format!(r#"<article class="session-card"><div class="session-top"><h3>{}</h3>{}</div><p>{matchup}</p><a href="/games/{}">Observe match →</a></article>"#,
            html_escape(&game.game_type), render_status_badge(&game.status), html_escape(&game.game_id)));
    }
    html.push_str("</div>");
    html
}

pub(in crate::server) fn render_game_fragment(
    game: &GameSummary,
    events: &[GameEvent],
) -> Result<String, AppError> {
    let GameSummary {
        game_id,
        game_type,
        agents,
        status,
        result,
    } = game;
    let live = matches!(status, GameStatus::Pending | GameStatus::Running);
    let poll = if live {
        format!(
            r#" hx-get="/games/{}/fragment" hx-trigger="every 500ms" hx-swap="outerHTML""#,
            html_escape(game_id)
        )
    } else {
        String::new()
    };
    let mut html = format!(
        r#"<div id="game-detail"{poll}>
            <div class="panel-heading">
                <div>
                    <p class="eyebrow">match detail</p>
                    <h2>{}</h2>
                    <div class="session-id">{}</div>
                </div>
                {}
            </div>"#,
        html_escape(game_type),
        html_escape(game_id),
        render_status_badge(status)
    );

    if agents.len() >= 2 {
        html.push_str(&render_player_legend(game_type, agents));
    }

    match result {
        Some(result) => {
            html.push_str(&render_result_detail(game_type, agents, result));
            if !result.stats().turns.is_empty() {
                html.push_str(&format!(r#"<p class="actions"><a class="button" href="/games/{}/replay">Replay match</a></p>"#, html_escape(game_id)));
            }
        }
        None => {
            let message = match status {
                GameStatus::Pending => "This game is queued and will start shortly.",
                GameStatus::Running => "Agents are battling. This panel refreshes every 500ms.",
                GameStatus::Completed => "The run completed without a stored result.",
                GameStatus::Error => {
                    "The run failed before producing a result. Check API keys, endpoints, or model availability."
                }
            };
            html.push_str(&format!("<div class=\"empty\">{}</div>", message));
            if let Some(event) = events.last() {
                if let Some(latest_state) = event.state() {
                    html.push_str(&render_human_state(game_type, agents, latest_state));
                } else if let GameEvent::Error { message, .. } = event {
                    html.push_str(&format!(
                        "<div class=\"notice error\">{}</div>",
                        html_escape(message)
                    ));
                }
            }
        }
    }

    let turns = match result {
        Some(result) => result
            .stats()
            .turns
            .iter()
            .map(|turn| &turn.state_after)
            .collect(),
        None => events.iter().filter_map(GameEvent::state).collect(),
    };
    BattleLog {
        game_id,
        turns,
        live,
    }
    .render_into(&mut html)
    .map_err(|error| AppError::Internal(error.to_string()))?;
    html.push_str("</div>");
    Ok(html)
}

fn render_result_detail(game_type: &str, agents: &[AIAgentConfig], result: &TestResult) -> String {
    let (stats, error) = result_stats(result);
    let outcome = if let Some(error) = error {
        format!("Error: {}", html_escape(error))
    } else if let Some(winner) = &stats.winner {
        format!("Winner: {}", html_escape(&display_winner(winner, agents)))
    } else if stats.draw {
        "Draw".to_string()
    } else {
        "Incomplete".to_string()
    };

    let mut html = format!(
        r#"<div class="result-grid">
            <div class="stat"><span>Outcome</span><strong>{}</strong></div>
            <div class="stat"><span>Turns</span><strong>{}</strong></div>
            <div class="stat"><span>Invalid Moves</span><strong>{}</strong></div>
            <div class="stat"><span>Avg Turn</span><strong>{:.0}ms</strong></div>
        </div>"#,
        outcome,
        stats.total_turns(),
        stats.invalid_moves,
        stats.average_turn_time_ms()
    );

    if let Some(last_turn) = stats.turns.last() {
        html.push_str(&render_human_state(
            game_type,
            agents,
            &last_turn.state_after,
        ));
    }

    if !stats.turns.is_empty() {
        html.push_str("<h3>Turn Log</h3><table><thead><tr><th>Turn</th><th>Agent</th><th>Move</th><th>Time</th><th>Status</th></tr></thead><tbody>");
        for turn in stats.turns.iter().rev().take(12).rev() {
            html.push_str(&format!(
                "<tr><td>{}</td><td>{}</td><td><code>{}</code></td><td>{}ms</td><td>{}</td></tr>",
                turn.turn_number,
                html_escape(&display_turn_player(&turn.player, agents)),
                html_escape(&turn.move_made.to_string()),
                turn.time_taken_ms,
                if turn.move_valid { "valid" } else { "invalid" }
            ));
        }
        html.push_str("</tbody></table>");
    }

    html
}

fn result_stats(result: &TestResult) -> (&GameStats, Option<&str>) {
    match result {
        TestResult::TicTacToe(result) => (&result.stats, result.error.as_deref()),
        TestResult::RockPaperScissors(result) => (&result.stats, result.error.as_deref()),
        TestResult::ConnectFour(result) => (&result.stats, result.error.as_deref()),
        TestResult::Chess(result) => (&result.stats, result.error.as_deref()),
        TestResult::Checkers(result) => (&result.stats, result.error.as_deref()),
        TestResult::Battleship(result) => (&result.stats, result.error.as_deref()),
    }
}

fn display_winner(winner: &str, agents: &[AIAgentConfig]) -> String {
    let Some((agent_index, suffix)) = parse_winner_agent_index(winner) else {
        return winner.to_string();
    };

    let Some(agent) = agents.get(agent_index) else {
        return winner.to_string();
    };

    if suffix.is_empty() {
        agent.model.clone()
    } else {
        format!("{} {}", agent.model, suffix)
    }
}

fn parse_winner_agent_index(winner: &str) -> Option<(usize, &str)> {
    let underscore = winner.find('_')?;
    let after_underscore = &winner[underscore + 1..];
    let digit_count = after_underscore
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .count();
    if digit_count == 0 {
        return None;
    }

    let player_number = after_underscore[..digit_count].parse::<usize>().ok()?;
    let suffix = after_underscore[digit_count..].trim_start();
    Some((player_number.checked_sub(1)?, suffix))
}

fn display_turn_player(player: &str, agents: &[AIAgentConfig]) -> String {
    if let Some((index, suffix)) = parse_winner_agent_index(player) {
        let role = suffix.trim();
        let role = role.trim_start_matches('(').trim_end_matches(')');
        if let Some(agent) = agents.get(index) {
            if role.is_empty() {
                return agent.model.clone();
            }
            return format!("{} ({})", agent.model, role);
        }
    }

    if player.starts_with("Player 1") && !agents.is_empty() {
        return agents[0].model.clone();
    }
    if player.starts_with("Player 2") && agents.len() > 1 {
        return agents[1].model.clone();
    }

    display_winner(player, agents)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_config::AgentKind;

    #[test]
    fn winner_display_uses_model_name_instead_of_agent_name() {
        let agents = vec![
            AIAgentConfig {
                agent: AgentKind::OpenAI,
                model: "gpt-test".to_string(),
                temp: 0.7,
                seed: None,
                secret_profile: None,
                api_endpoint: None,
                prompt_file: None,
            },
            AIAgentConfig {
                agent: AgentKind::Ollama,
                model: "qwen3.6:latest".to_string(),
                temp: 0.7,
                seed: None,
                secret_profile: None,
                api_endpoint: Some("http://localhost:11434".to_string()),
                prompt_file: None,
            },
        ];

        assert_eq!(
            display_winner("Ollama_2 (O)", &agents),
            "qwen3.6:latest (O)"
        );
    }
}
