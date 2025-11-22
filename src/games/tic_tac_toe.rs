use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Instant;

use crate::agent::{AIAgent, MoveRequest, MoveResponse};
use crate::games::stats::{GameStats, TurnStats};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TicTacToeConfig {
    pub board_size: u32,
    pub win_length: u32,
}

impl Default for TicTacToeConfig {
    fn default() -> Self {
        TicTacToeConfig {
            board_size: 3,
            win_length: 3,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TicTacToeState {
    pub board: Vec<Vec<Option<Player>>>,
    pub current_player: Player,
    pub turn_number: u32,
    pub game_over: bool,
    pub winner: Option<Player>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Player {
    X,
    O,
}

impl Player {
    fn as_str(&self) -> &str {
        match self {
            Player::X => "X",
            Player::O => "O",
        }
    }

    fn to_string(&self) -> String {
        self.as_str().to_string()
    }

    fn other(&self) -> Player {
        match self {
            Player::X => Player::O,
            Player::O => Player::X,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TicTacToeMove {
    pub row: u32,
    pub col: u32,
}

pub struct TicTacToe {
    config: TicTacToeConfig,
    state: TicTacToeState,
    stats: GameStats,
    game_id: String,
}

impl TicTacToe {
    pub fn new(config: TicTacToeConfig) -> Self {
        let board_size = config.board_size as usize;
        let board = vec![vec![None; board_size]; board_size];
        
        Self {
            config,
            state: TicTacToeState {
                board,
                current_player: Player::X,
                turn_number: 0,
                game_over: false,
                winner: None,
            },
            stats: GameStats::new(),
            game_id: format!("ttt_{}", uuid::Uuid::new_v4().to_string()[..8].to_string()),
        }
    }

    pub async fn play_game(mut self, agents: Vec<AIAgent>) -> TicTacToeResult {
        let start_time = Instant::now();
        
        // Ensure we have exactly 2 agents
        if agents.len() != 2 {
            return TicTacToeResult {
                winner: None,
                stats: self.stats,
                error: Some(format!("Expected 2 agents, got {}", agents.len())),
            };
        }

        let player_x_agent = &agents[0];
        let player_o_agent = &agents[1];
        
        // Map players to agents
        let agent_map: Vec<(&AIAgent, Player)> = vec![
            (player_x_agent, Player::X),
            (player_o_agent, Player::O),
        ];

        while !self.state.game_over && self.state.turn_number < (self.config.board_size * self.config.board_size) {
            let current_agent_idx = match self.state.current_player {
                Player::X => 0,
                Player::O => 1,
            };
            
            let (agent, player) = &agent_map[current_agent_idx];
            
            // Execute turn
            match self.execute_turn(agent, *player).await {
                Ok(()) => {
                    // Check for win condition
                    if self.check_win() {
                        self.state.game_over = true;
                        self.state.winner = Some(self.state.current_player);
                        self.stats.winner = Some(format!("{} ({})", agent.name(), self.state.current_player.as_str()));
                        break;
                    }
                    
                    // Check for draw
                    if self.state.turn_number >= (self.config.board_size * self.config.board_size) {
                        self.state.game_over = true;
                        self.stats.draw = true;
                        break;
                    }
                    
                    // Switch player
                    self.state.current_player = self.state.current_player.other();
                }
                Err(e) => {
                    // Invalid move - game continues but stats are tracked
                    eprintln!("Turn error: {}", e);
                }
            }
        }

        let total_duration = start_time.elapsed();
        self.stats.total_duration_ms = total_duration.as_millis() as u64;

        TicTacToeResult {
            winner: self.stats.winner.clone(),
            stats: self.stats,
            error: None,
        }
    }

    async fn execute_turn(&mut self, agent: &AIAgent, player: Player) -> Result<(), String> {
        let turn_start = Instant::now();
        self.state.turn_number += 1;

        // Create game state JSON
        let state_json = self.state_to_json();
        let state_before = state_json.clone();

        // Create move schema
        let move_schema = json!({
            "type": "object",
            "properties": {
                "row": {
                    "type": "integer",
                    "minimum": 0,
                    "maximum": self.config.board_size - 1,
                    "description": "Row index (0-indexed)"
                },
                "col": {
                    "type": "integer",
                    "minimum": 0,
                    "maximum": self.config.board_size - 1,
                    "description": "Column index (0-indexed)"
                }
            },
            "required": ["row", "col"]
        });

        // Create move request
        let move_request = MoveRequest {
            turn_index: self.state.turn_number,
            game_id: self.game_id.clone(),
            state: state_json,
            expected_move_schema: move_schema,
        };

        // Get move from agent
        let move_response: MoveResponse = agent
            .execute_turn(&move_request)
            .await
            .map_err(|e| format!("Agent error: {}", e))?;

        let time_taken = turn_start.elapsed();

        // Parse move
        let move_data = move_response.chosen_move;
        let row = move_data
            .get("row")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| "Missing or invalid 'row' field".to_string())? as u32;
        let col = move_data
            .get("col")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| "Missing or invalid 'col' field".to_string())? as u32;

        // Validate move
        let move_valid = self.is_valid_move(row, col);
        let error_message = if !move_valid {
            Some(format!("Invalid move: row={}, col={}", row, col))
        } else {
            None
        };

        // Apply move if valid
        let state_after = if move_valid {
            self.state.board[row as usize][col as usize] = Some(player);
            self.state_to_json()
        } else {
            state_before.clone()
        };

        // Record turn stats
        let turn_stats = TurnStats {
            turn_number: self.state.turn_number,
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
            return Err(error_message.unwrap_or_else(|| "Invalid move".to_string()));
        }

        Ok(())
    }

    fn is_valid_move(&self, row: u32, col: u32) -> bool {
        if row >= self.config.board_size || col >= self.config.board_size {
            return false;
        }
        self.state.board[row as usize][col as usize].is_none()
    }

    fn check_win(&self) -> bool {
        let board_size = self.config.board_size as usize;
        let win_length = self.config.win_length as usize;
        let player = self.state.current_player;

        // Check rows
        for row in 0..board_size {
            let mut count = 0;
            for col in 0..board_size {
                if self.state.board[row][col] == Some(player) {
                    count += 1;
                    if count >= win_length {
                        return true;
                    }
                } else {
                    count = 0;
                }
            }
        }

        // Check columns
        for col in 0..board_size {
            let mut count = 0;
            for row in 0..board_size {
                if self.state.board[row][col] == Some(player) {
                    count += 1;
                    if count >= win_length {
                        return true;
                    }
                } else {
                    count = 0;
                }
            }
        }

        // Check diagonals (top-left to bottom-right)
        for start_row in 0..=board_size.saturating_sub(win_length) {
            for start_col in 0..=board_size.saturating_sub(win_length) {
                let mut count = 0;
                for i in 0..win_length {
                    let row = start_row + i;
                    let col = start_col + i;
                    if row < board_size && col < board_size && self.state.board[row][col] == Some(player) {
                        count += 1;
                        if count >= win_length {
                            return true;
                        }
                    } else {
                        break;
                    }
                }
            }
        }

        // Check diagonals (top-right to bottom-left)
        for start_row in 0..=board_size.saturating_sub(win_length) {
            for start_col in (win_length - 1)..board_size {
                let mut count = 0;
                for i in 0..win_length {
                    let row = start_row + i;
                    let col = start_col.saturating_sub(i);
                    if row < board_size && col < board_size && self.state.board[row][col] == Some(player) {
                        count += 1;
                        if count >= win_length {
                            return true;
                        }
                    } else {
                        break;
                    }
                }
            }
        }

        false
    }

    fn state_to_json(&self) -> Value {
        let board: Vec<Vec<Option<String>>> = self
            .state
            .board
            .iter()
            .map(|row| {
                row.iter()
                    .map(|cell| cell.map(|p| p.to_string()))
                    .collect()
            })
            .collect();

        json!({
            "board": board,
            "current_player": self.state.current_player.to_string(),
            "turn_number": self.state.turn_number,
            "game_over": self.state.game_over,
            "winner": self.state.winner.map(|p| p.to_string()),
            "board_size": self.config.board_size,
            "win_length": self.config.win_length,
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TicTacToeResult {
    pub winner: Option<String>,
    pub stats: GameStats,
    pub error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_player_as_str() {
        assert_eq!(Player::X.as_str(), "X");
        assert_eq!(Player::O.as_str(), "O");
    }

    #[test]
    fn test_player_to_string() {
        assert_eq!(Player::X.to_string(), "X");
        assert_eq!(Player::O.to_string(), "O");
    }

    #[test]
    fn test_player_other() {
        assert_eq!(Player::X.other(), Player::O);
        assert_eq!(Player::O.other(), Player::X);
    }

    #[test]
    fn test_tic_tac_toe_new() {
        let config = TicTacToeConfig::default();
        let game = TicTacToe::new(config);
        
        assert_eq!(game.state.board.len(), 3);
        assert_eq!(game.state.board[0].len(), 3);
        assert_eq!(game.state.current_player, Player::X);
        assert_eq!(game.state.turn_number, 0);
        assert_eq!(game.state.game_over, false);
        assert_eq!(game.state.winner, None);
    }

    #[test]
    fn test_tic_tac_toe_new_custom_size() {
        let config = TicTacToeConfig {
            board_size: 5,
            win_length: 4,
        };
        let game = TicTacToe::new(config);
        
        assert_eq!(game.state.board.len(), 5);
        assert_eq!(game.state.board[0].len(), 5);
    }

    #[test]
    fn test_is_valid_move_empty_board() {
        let config = TicTacToeConfig::default();
        let game = TicTacToe::new(config);
        
        assert!(game.is_valid_move(0, 0));
        assert!(game.is_valid_move(1, 1));
        assert!(game.is_valid_move(2, 2));
    }

    #[test]
    fn test_is_valid_move_out_of_bounds() {
        let config = TicTacToeConfig::default();
        let game = TicTacToe::new(config);
        
        assert!(!game.is_valid_move(3, 0));
        assert!(!game.is_valid_move(0, 3));
        assert!(!game.is_valid_move(10, 10));
    }

    #[test]
    fn test_is_valid_move_occupied() {
        let config = TicTacToeConfig::default();
        let mut game = TicTacToe::new(config);
        
        // Place a piece
        game.state.board[1][1] = Some(Player::X);
        
        assert!(!game.is_valid_move(1, 1));
        assert!(game.is_valid_move(0, 0));
        assert!(game.is_valid_move(2, 2));
    }

    #[test]
    fn test_check_win_horizontal() {
        let config = TicTacToeConfig::default();
        let mut game = TicTacToe::new(config);
        
        // Create horizontal win for X
        game.state.board[0][0] = Some(Player::X);
        game.state.board[0][1] = Some(Player::X);
        game.state.board[0][2] = Some(Player::X);
        game.state.current_player = Player::X;
        
        assert!(game.check_win());
    }

    #[test]
    fn test_check_win_vertical() {
        let config = TicTacToeConfig::default();
        let mut game = TicTacToe::new(config);
        
        // Create vertical win for O
        game.state.board[0][1] = Some(Player::O);
        game.state.board[1][1] = Some(Player::O);
        game.state.board[2][1] = Some(Player::O);
        game.state.current_player = Player::O;
        
        assert!(game.check_win());
    }

    #[test]
    fn test_check_win_diagonal_tl_br() {
        let config = TicTacToeConfig::default();
        let mut game = TicTacToe::new(config);
        
        // Create diagonal win (top-left to bottom-right)
        game.state.board[0][0] = Some(Player::X);
        game.state.board[1][1] = Some(Player::X);
        game.state.board[2][2] = Some(Player::X);
        game.state.current_player = Player::X;
        
        assert!(game.check_win());
    }

    #[test]
    fn test_check_win_diagonal_tr_bl() {
        let config = TicTacToeConfig::default();
        let mut game = TicTacToe::new(config);
        
        // Create diagonal win (top-right to bottom-left)
        game.state.board[0][2] = Some(Player::O);
        game.state.board[1][1] = Some(Player::O);
        game.state.board[2][0] = Some(Player::O);
        game.state.current_player = Player::O;
        
        assert!(game.check_win());
    }

    #[test]
    fn test_check_win_no_win() {
        let config = TicTacToeConfig::default();
        let mut game = TicTacToe::new(config);
        
        // Partial game, no win
        game.state.board[0][0] = Some(Player::X);
        game.state.board[0][1] = Some(Player::O);
        game.state.board[1][1] = Some(Player::X);
        game.state.current_player = Player::X;
        
        assert!(!game.check_win());
    }

    #[test]
    fn test_check_win_custom_win_length() {
        let config = TicTacToeConfig {
            board_size: 5,
            win_length: 4,
        };
        let mut game = TicTacToe::new(config);
        
        // Create horizontal win of length 4
        game.state.board[2][0] = Some(Player::X);
        game.state.board[2][1] = Some(Player::X);
        game.state.board[2][2] = Some(Player::X);
        game.state.board[2][3] = Some(Player::X);
        game.state.current_player = Player::X;
        
        assert!(game.check_win());
    }

    #[test]
    fn test_state_to_json() {
        let config = TicTacToeConfig::default();
        let mut game = TicTacToe::new(config);
        game.state.board[0][0] = Some(Player::X);
        game.state.turn_number = 1;
        
        let json = game.state_to_json();
        assert_eq!(json["board"][0][0], "X");
        assert_eq!(json["turn_number"], 1);
        assert_eq!(json["board_size"], 3);
        assert_eq!(json["win_length"], 3);
    }

    #[test]
    fn test_config_default() {
        let config = TicTacToeConfig::default();
        assert_eq!(config.board_size, 3);
        assert_eq!(config.win_length, 3);
    }
}

