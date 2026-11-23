use chess::{Board, ChessMove, Color, Game as ChessGame, GameResult, MoveGen};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::str::FromStr;
use std::time::Instant;

use crate::agent::{AIAgent, MoveRequest};
use crate::games::events::GameEvent;
use crate::games::stats::{GameStats, TurnStats};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChessConfig {
    #[serde(default)]
    pub start_fen: Option<String>,
}

impl Default for ChessConfig {
    fn default() -> Self {
        ChessConfig { start_fen: None }
    }
}

pub struct Chess {
    config: ChessConfig,
    game: ChessGame,
    stats: GameStats,
    game_id: String,
    last_move: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChessResult {
    pub winner: Option<String>,
    pub stats: GameStats,
    pub error: Option<String>,
}

impl Chess {
    pub fn new(config: ChessConfig) -> Self {
        let game = if let Some(fen) = &config.start_fen {
            let board = Board::from_str(fen).expect("Invalid FEN string");
            ChessGame::new_with_board(board)
        } else {
            ChessGame::new()
        };

        Self {
            config,
            game,
            stats: GameStats::new(),
            game_id: format!(
                "chess_{}",
                uuid::Uuid::new_v4().to_string()[..8].to_string()
            ),
            last_move: None,
        }
    }

    pub async fn play_game(
        mut self,
        agents: Vec<AIAgent>,
        observer: Option<tokio::sync::mpsc::Sender<GameEvent>>,
    ) -> ChessResult {
        let start_time = Instant::now();

        if agents.len() != 2 {
            return ChessResult {
                winner: None,
                stats: self.stats,
                error: Some(format!("Expected 2 agents, got {}", agents.len())),
            };
        }

        let white_agent = &agents[0];
        let black_agent = &agents[1];

        let mut turn_count = 0;
        // Cap max turns to prevent infinite games
        let max_turns = 200;

        // TODO: Track these in stats
        let mut invalid_back_moves = 0;
        let mut invalid_white_moves = 0;

        while self.game.result().is_none() && turn_count < max_turns {
            turn_count += 1;
            let current_color = self.game.current_position().side_to_move();
            let agent = match current_color {
                Color::White => white_agent,
                Color::Black => black_agent,
            };

            let mut valid = false;
            while !valid {
                match self.execute_turn(agent, turn_count).await {
                    Ok(_) => {
                        // Send state update if observer is present
                        valid = true;
                        if let Some(observer) = &observer {
                            let event = GameEvent::turn(
                                "Chess",
                                turn_count,
                                self.state_to_json(turn_count),
                            );
                            let _ = observer.send(event).await;
                        }
                    }
                    Err(e) => {
                        eprintln!("Turn error: {:?} agent {}", current_color, e);
                        turn_count += 1;
                        match current_color {
                            Color::White => invalid_white_moves += 1,
                            Color::Black => invalid_back_moves += 1,
                        }
                        // If an agent makes an invalid move, they lose?
                        // For now, we'll just log it and maybe break or continue?
                        // In this framework, usually we might want to count it as a loss or just stop.
                        // Let's stop and mark error.

                        if invalid_back_moves >= 10 {
                            return ChessResult {
                                winner: Some("White".to_string()), // Or maybe the other player?
                                stats: self.stats,
                                error: Some(format!("Game ended due to error: {}", e)),
                            };
                        }

                        if invalid_white_moves >= 10 {
                            return ChessResult {
                                winner: Some("Black".to_string()), // Or maybe the other player?
                                stats: self.stats,
                                error: Some(format!("Game ended due to error: {}", e)),
                            };
                        }
                    }
                }
            }
        }

        let total_duration = start_time.elapsed();
        self.stats.total_duration_ms = total_duration.as_millis() as u64;

        let winner = match self.game.result() {
            Some(GameResult::WhiteCheckmates) | Some(GameResult::BlackResigns) => {
                Some(format!("{} (White)", white_agent.name()))
            }
            Some(GameResult::BlackCheckmates) | Some(GameResult::WhiteResigns) => {
                Some(format!("{} (Black)", black_agent.name()))
            }
            Some(GameResult::DrawAccepted)
            | Some(GameResult::DrawDeclared)
            | Some(GameResult::Stalemate) => None, // Draw
            None => None, // Max turns reached or other
        };

        if winner.is_none() && self.game.result().is_some() {
            self.stats.draw = true;
        }

        self.stats.winner = winner.clone();

        ChessResult {
            winner,
            stats: self.stats,
            error: None,
        }
    }

    async fn execute_turn(&mut self, agent: &AIAgent, turn_number: u32) -> Result<(), String> {
        let turn_start = Instant::now();

        let state_json = self.state_to_json(turn_number);
        let state_before = state_json.clone();

        // Create move schema
        let move_schema = json!({
            "type": "object",
            "properties": {
                "move": {
                    "type": "string",
                    "description": "The move in UCI format (e.g., 'e2e4', 'a7a8q')"
                }
            },
            "required": ["move"]
        });

        let move_request = MoveRequest {
            turn_index: turn_number,
            game_id: self.game_id.clone(),
            state: state_json,
            expected_move_schema: move_schema,
            instructions: Some(
                "Please output your move in UCI format (e.g., e2e4, a7a8q).".to_string(),
            ),
        };

        let move_response = agent
            .execute_turn(&move_request)
            .await
            .map_err(|e| format!("Agent error: {}", e))?;

        let time_taken = turn_start.elapsed();

        let move_data = move_response.chosen_move;
        let move_str = move_data
            .get("move")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "Missing or invalid 'move' field".to_string())?;

        // Parse and validate move
        /// let mut movegen = MoveGen::new_legal(&game.current_position());
        ///
        /// game.make_move(movegen.next().expect("At least one legal move"));
        let chess_move = ChessMove::from_str(move_str)
            .map_err(|_| format!("Invalid UCI move format: {}", move_str))?;

        // Check if move is legal
        let board = self.game.current_position();
        let move_valid = board.legal(chess_move);

        let error_message = if !move_valid {
            Some(format!("Illegal move: {}", move_str))
        } else {
            None
        };

        // Apply move if valid
        let state_after = if move_valid {
            self.game.make_move(chess_move);
            self.last_move = Some(move_str.to_string());
            self.state_to_json(turn_number + 1)
        } else {
            state_before.clone()
        };

        let turn_stats = TurnStats {
            turn_number,
            player: agent.name().to_string(),
            move_made: move_data.clone(),
            time_taken_ms: time_taken.as_millis() as u64,
            move_valid,
            error_message: error_message.clone(),
            state_before,
            state_after,
            diagnostics: move_response.diagnostics,
        };

        self.stats.add_turn(turn_stats);

        if !move_valid {
            return Err(error_message.unwrap());
        }

        Ok(())
    }

    fn state_to_json(&self, turn_number: u32) -> Value {
        let board = self.game.current_position();
        json!({
            "fen": format!("{}", board),
            "turn_number": turn_number,
            "current_player": match board.side_to_move() {
                Color::White => "White",
                Color::Black => "Black",
            },
            "is_check": board.checkers().popcnt() > 0,
            "last_move": self.last_move,
            // We could add more info like legal moves if we wanted to help the agent
            "legal_moves": MoveGen::new_legal(&board).map(|m| m.to_string()).collect::<Vec<String>>(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chess_new() {
        let config = ChessConfig::default();
        let game = Chess::new(config);
        assert_eq!(game.game.current_position().side_to_move(), Color::White);
    }

    #[test]
    fn test_chess_fen() {
        let config = ChessConfig {
            start_fen: Some(
                "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1".to_string(),
            ),
        };
        let game = Chess::new(config);
        assert_eq!(game.game.current_position().side_to_move(), Color::Black);
    }
}
