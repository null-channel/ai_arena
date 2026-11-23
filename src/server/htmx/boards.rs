use super::{forms::kind_name, layout::html_escape};
use crate::agent_config::AIAgentConfig;

pub(super) fn render_human_state(
    game_type: &str,
    agents: &[AIAgentConfig],
    state: &serde_json::Value,
) -> String {
    match game_type {
        "TicTacToe" | "ConnectFour" | "Checkers" => render_grid_board(game_type, agents, state),
        "Chess" => render_chess_board(agents, state),
        "RockPaperScissors" => render_rps_state(agents, state),
        "Battleship" => render_state_fallback(state),
        _ => render_state_fallback(state),
    }
}

struct PlayerSlot<'a> {
    agent_index: usize,
    role_label: &'a str,
    mark: &'a str,
    piece_class: &'a str,
}

fn player_slots_for_game(game_type: &str) -> Vec<PlayerSlot<'static>> {
    match game_type {
        "TicTacToe" => vec![
            PlayerSlot {
                agent_index: 0,
                role_label: "plays X",
                mark: "X",
                piece_class: "piece-x",
            },
            PlayerSlot {
                agent_index: 1,
                role_label: "plays O",
                mark: "O",
                piece_class: "piece-o",
            },
        ],
        "ConnectFour" => vec![
            PlayerSlot {
                agent_index: 0,
                role_label: "Red discs",
                mark: "●",
                piece_class: "piece-red",
            },
            PlayerSlot {
                agent_index: 1,
                role_label: "Yellow discs",
                mark: "●",
                piece_class: "piece-yellow",
            },
        ],
        "Checkers" => vec![
            PlayerSlot {
                agent_index: 0,
                role_label: "Black pieces",
                mark: "●",
                piece_class: "piece-red",
            },
            PlayerSlot {
                agent_index: 1,
                role_label: "Red pieces",
                mark: "●",
                piece_class: "piece-yellow",
            },
        ],
        "Chess" => vec![
            PlayerSlot {
                agent_index: 0,
                role_label: "White",
                mark: "♚",
                piece_class: "piece-white",
            },
            PlayerSlot {
                agent_index: 1,
                role_label: "Black",
                mark: "♚",
                piece_class: "piece-black",
            },
        ],
        "RockPaperScissors" => vec![
            PlayerSlot {
                agent_index: 0,
                role_label: "Player 1",
                mark: "P1",
                piece_class: "",
            },
            PlayerSlot {
                agent_index: 1,
                role_label: "Player 2",
                mark: "P2",
                piece_class: "",
            },
        ],
        _ => Vec::new(),
    }
}

pub(super) fn render_player_legend(game_type: &str, agents: &[AIAgentConfig]) -> String {
    let slots = player_slots_for_game(game_type);
    if slots.is_empty() || agents.len() < 2 {
        return String::new();
    }

    let mut html = String::from("<div class=\"player-legend\">");
    for slot in slots {
        let agent = &agents[slot.agent_index];
        html.push_str(&format!(
            r#"<div class="player-chip">
                <span class="player-mark {}">{}</span>
                <span class="player-info">
                    <strong>{}</strong>
                    <small>{} · {}</small>
                </span>
            </div>"#,
            slot.piece_class,
            html_escape(slot.mark),
            html_escape(&agent.model),
            html_escape(slot.role_label),
            html_escape(kind_name(agent.agent))
        ));
    }
    html.push_str("</div>");
    html
}

fn current_player_display(game_type: &str, current: &str, agents: &[AIAgentConfig]) -> String {
    match game_type {
        "TicTacToe" => match current {
            "X" if !agents.is_empty() => format!("{} (X)", agents[0].model),
            "O" if agents.len() > 1 => format!("{} (O)", agents[1].model),
            _ => current.to_string(),
        },
        "ConnectFour" => match current {
            "Red" if !agents.is_empty() => format!("{} (Red)", agents[0].model),
            "Yellow" if agents.len() > 1 => format!("{} (Yellow)", agents[1].model),
            _ => current.to_string(),
        },
        "Chess" => match current {
            "White" if !agents.is_empty() => format!("{} (White)", agents[0].model),
            "Black" if agents.len() > 1 => format!("{} (Black)", agents[1].model),
            _ => current.to_string(),
        },
        "RockPaperScissors" => match current {
            "Player 1" if !agents.is_empty() => agents[0].model.clone(),
            "Player 2" if agents.len() > 1 => agents[1].model.clone(),
            _ => current.to_string(),
        },
        _ => current.to_string(),
    }
}

fn render_grid_board(
    game_type: &str,
    agents: &[AIAgentConfig],
    state: &serde_json::Value,
) -> String {
    let Some(rows) = state.get("board").and_then(|board| board.as_array()) else {
        return render_state_fallback(state);
    };

    let current_player = state
        .get("current_player")
        .and_then(|value| value.as_str())
        .unwrap_or("unknown");
    let turn = state
        .get("turn_number")
        .and_then(|value| value.as_u64())
        .unwrap_or_default();

    let current_label = current_player_display(game_type, current_player, agents);
    let mut html = format!(
        "<div class=\"board-wrap\"><h3>Board</h3><p class=\"board-meta\">Turn {} · to move: {}</p><table class=\"game-board\"><tbody>",
        turn,
        html_escape(&current_label)
    );

    for row in rows {
        html.push_str("<tr>");
        if let Some(cells) = row.as_array() {
            for cell in cells {
                let piece = cell.as_str().unwrap_or("");
                let label = board_piece_label(piece);
                html.push_str(&format!(
                    "<td class=\"{}\">{}</td>",
                    board_piece_class(piece),
                    label
                ));
            }
        }
        html.push_str("</tr>");
    }

    html.push_str("</tbody></table></div>");
    html
}

fn board_piece_label(piece: &str) -> String {
    match piece {
        "" => "·".to_string(),
        "X" => "X".to_string(),
        "O" => "O".to_string(),
        "Red" => "●".to_string(),
        "Yellow" => "●".to_string(),
        other => html_escape(other),
    }
}

fn board_piece_class(piece: &str) -> &'static str {
    match piece {
        "" => "cell-empty",
        "X" => "piece-x",
        "O" => "piece-o",
        "Red" => "piece-red",
        "Yellow" => "piece-yellow",
        _ => "",
    }
}

fn render_chess_board(agents: &[AIAgentConfig], state: &serde_json::Value) -> String {
    let Some(fen) = state.get("fen").and_then(|value| value.as_str()) else {
        return render_state_fallback(state);
    };

    let current_player = state
        .get("current_player")
        .and_then(|value| value.as_str())
        .unwrap_or("unknown");
    let board_part = fen.split_whitespace().next().unwrap_or(fen);
    let current_label = current_player_display("Chess", current_player, agents);
    let mut html = format!(
        "<div class=\"board-wrap\"><h3>Board</h3><p class=\"board-meta\">To move: {} · FEN: <code>{}</code></p><table class=\"game-board chess\"><tbody>",
        html_escape(&current_label),
        html_escape(fen)
    );

    for (rank_idx, rank) in board_part.split('/').enumerate() {
        html.push_str("<tr>");
        let mut file_idx = 0usize;
        for ch in rank.chars() {
            if let Some(empty_count) = ch.to_digit(10) {
                for _ in 0..empty_count {
                    html.push_str(&format!(
                        "<td class=\"{} cell-empty\">&nbsp;</td>",
                        chess_square_class(rank_idx, file_idx)
                    ));
                    file_idx += 1;
                }
            } else {
                html.push_str(&format!(
                    "<td class=\"{}\"><span class=\"{}\">{}</span></td>",
                    chess_square_class(rank_idx, file_idx),
                    chess_piece_color_class(ch),
                    chess_piece_symbol(ch)
                ));
                file_idx += 1;
            }
        }
        html.push_str("</tr>");
    }

    html.push_str("</tbody></table></div>");
    html
}

fn chess_square_class(rank_idx: usize, file_idx: usize) -> &'static str {
    if (rank_idx + file_idx) % 2 == 0 {
        "light"
    } else {
        "dark"
    }
}

// Always use the filled glyphs; side color comes from the piece-white/piece-black
// CSS classes. The outline glyphs (♔ etc.) render as wireframes and inherit the
// square's text color, which made white/black indistinguishable.
fn chess_piece_symbol(piece: char) -> &'static str {
    match piece.to_ascii_lowercase() {
        'k' => "♚",
        'q' => "♛",
        'r' => "♜",
        'b' => "♝",
        'n' => "♞",
        'p' => "♟",
        _ => "?",
    }
}

fn chess_piece_color_class(piece: char) -> &'static str {
    if piece.is_ascii_uppercase() {
        "piece-white"
    } else {
        "piece-black"
    }
}

fn render_rps_state(agents: &[AIAgentConfig], state: &serde_json::Value) -> String {
    let p1 = state
        .get("player_one_score")
        .and_then(|value| value.as_u64())
        .unwrap_or_default();
    let p2 = state
        .get("player_two_score")
        .and_then(|value| value.as_u64())
        .unwrap_or_default();
    let round = state
        .get("round")
        .and_then(|value| value.as_u64())
        .unwrap_or_default();

    let player_one = agents
        .first()
        .map(|a| a.model.as_str())
        .unwrap_or("Player 1");
    let player_two = agents
        .get(1)
        .map(|a| a.model.as_str())
        .unwrap_or("Player 2");
    let mut html = format!(
        "<div class=\"board-wrap\"><h3>Scoreboard</h3><p class=\"board-meta\">Round {} · {} {} - {} {}</p>",
        round,
        html_escape(player_one),
        p1,
        html_escape(player_two),
        p2
    );

    if let Some(history) = state
        .get("round_history")
        .and_then(|value| value.as_array())
    {
        html.push_str("<div class=\"rps-history\">");
        for item in history.iter().rev().take(5).rev() {
            let round_number = item
                .get("round_number")
                .and_then(|value| value.as_u64())
                .unwrap_or_default();
            let one = item
                .get("player_one_choice")
                .and_then(|value| value.as_str())
                .unwrap_or("-");
            let two = item
                .get("player_two_choice")
                .and_then(|value| value.as_str())
                .unwrap_or("-");
            html.push_str(&format!(
                "<div class=\"rps-round\"><strong>Round {}</strong><span>{} vs {}</span></div>",
                round_number,
                html_escape(one),
                html_escape(two)
            ));
        }
        html.push_str("</div>");
    }

    html.push_str("</div>");
    html
}

fn render_state_fallback(state: &serde_json::Value) -> String {
    format!(
        "<div class=\"board-wrap\"><h3>State</h3><pre>{}</pre></div>",
        html_escape(&serde_json::to_string_pretty(state).unwrap_or_default())
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_config::AgentKind;
    use serde_json::json;

    #[test]
    fn board_state_renders_human_grid_not_json_dump() {
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
                api_endpoint: None,
                prompt_file: None,
            },
        ];
        let html = render_human_state(
            "TicTacToe",
            &agents,
            &json!({
                "board": [["X", null, "O"], [null, "X", null], ["O", null, "X"]],
                "current_player": "O",
                "turn_number": 5
            }),
        );

        assert!(html.contains("class=\"game-board\""));
        assert!(html.contains("piece-x"));
        assert!(html.contains("qwen3.6:latest (O)"));
        assert!(!html.contains("<pre>"));
    }

    #[test]
    fn player_legend_maps_symbols_to_models() {
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
                api_endpoint: None,
                prompt_file: None,
            },
        ];

        let html = render_player_legend("TicTacToe", &agents);
        assert!(html.contains("plays X"));
        assert!(html.contains("plays O"));
        assert!(html.contains("gpt-test"));
        assert!(html.contains("qwen3.6:latest"));
    }
}
