use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Instant;

use crate::agent::{AIAgent, MoveRequest, MoveResponse};
use crate::games::stats::{GameStats, TurnStats};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConnectFourConfig {
    pub rows: u32,
    pub cols: u32,
    pub win_length: u32,
}

impl Default for ConnectFourConfig {
    fn default() -> Self {
        ConnectFourConfig {
            rows: 6,
            cols: 7,
            win_length: 4,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConnectFourState {
    pub board: Vec<Vec<Option<Player>>>,
    pub current_player: Player,
    pub turn_number: u32,
    pub game_over: bool,
    pub winner: Option<Player>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Player {
    Red,
    Yellow,
}

impl Player {
    fn as_str(&self) -> &str {
        match self {
            Player::Red => "Red",
            Player::Yellow => "Yellow",
        }
    }

    fn to_string(&self) -> String {
        self.as_str().to_string()
    }

    fn other(&self) -> Player {
        match self {
            Player::Red => Player::Yellow,
            Player::Yellow => Player::Red,
        }
    }
}

pub struct ConnectFour {
    config: ConnectFourConfig,
    state: ConnectFourState,
    stats: GameStats,
    game_id: String,
}

impl ConnectFour {
    pub fn new(config: ConnectFourConfig) -> Self {
        let rows = config.rows as usize;
        let cols = config.cols as usize;
        let board = vec![vec![None; cols]; rows];
        
        Self {
            config,
            state: ConnectFourState {
                board,
                current_player: Player::Red,
                turn_number: 0,
                game_over: false,
                winner: None,
            },
            stats: GameStats::new(),
            game_id: format!("c4_{}", uuid::Uuid::new_v4().to_string()[..8].to_string()),
        }
    }

    pub async fn play_game(mut self, agents: Vec<AIAgent>) -> ConnectFourResult {
        let start_time = Instant::now();
        
        // Ensure we have exactly 2 agents
        if agents.len() != 2 {
            return ConnectFourResult {
                winner: None,
                stats: self.stats,
                error: Some(format!("Expected 2 agents, got {}", agents.len())),
            };
        }

        let player_red_agent = &agents[0];
        let player_yellow_agent = &agents[1];
        
        // Map players to agents
        let agent_map: Vec<(&AIAgent, Player)> = vec![
            (player_red_agent, Player::Red),
            (player_yellow_agent, Player::Yellow),
        ];

        let max_turns = self.config.rows * self.config.cols;
        
        while !self.state.game_over && self.state.turn_number < max_turns {
            let current_agent_idx = match self.state.current_player {
                Player::Red => 0,
                Player::Yellow => 1,
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
                    
                    // Check for draw (board full)
                    if self.state.turn_number >= max_turns {
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

        ConnectFourResult {
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
                "column": {
                    "type": "integer",
                    "minimum": 0,
                    "maximum": self.config.cols - 1,
                    "description": "Column index (0-indexed) where to drop the piece"
                }
            },
            "required": ["column"]
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
        let column = move_data
            .get("column")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| "Missing or invalid 'column' field".to_string())? as u32;

        // Validate and apply move
        let move_valid = self.is_valid_move(column);
        let error_message = if !move_valid {
            Some(format!("Invalid move: column={} (column full or out of bounds)", column))
        } else {
            None
        };

        // Apply move if valid
        let state_after = if move_valid {
            self.drop_piece(column, player);
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

    fn is_valid_move(&self, column: u32) -> bool {
        if column >= self.config.cols {
            return false;
        }
        // Check if column has space (top row is empty)
        self.state.board[0][column as usize].is_none()
    }

    fn drop_piece(&mut self, column: u32, player: Player) {
        let col = column as usize;
        let rows = self.config.rows as usize;
        
        // Find the lowest empty row in the column
        for row in (0..rows).rev() {
            if self.state.board[row][col].is_none() {
                self.state.board[row][col] = Some(player);
                return;
            }
        }
    }

    fn check_win(&self) -> bool {
        let rows = self.config.rows as usize;
        let cols = self.config.cols as usize;
        let win_length = self.config.win_length as usize;
        let player = self.state.current_player;

        // Check horizontal
        for row in 0..rows {
            let mut count = 0;
            for col in 0..cols {
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

        // Check vertical
        for col in 0..cols {
            let mut count = 0;
            for row in 0..rows {
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

        // Check diagonal (top-left to bottom-right)
        for start_row in 0..=rows.saturating_sub(win_length) {
            for start_col in 0..=cols.saturating_sub(win_length) {
                let mut count = 0;
                for i in 0..win_length {
                    let row = start_row + i;
                    let col = start_col + i;
                    if row < rows && col < cols && self.state.board[row][col] == Some(player) {
                        count += 1;
                        if count >= win_length {
                            return true;
                        }
                    } else {
                        count = 0;
                    }
                }
            }
        }

        // Check diagonal (top-right to bottom-left)
        for start_row in 0..=rows.saturating_sub(win_length) {
            for start_col in (win_length - 1)..cols {
                let mut count = 0;
                for i in 0..win_length {
                    let row = start_row + i;
                    let col = start_col.saturating_sub(i);
                    if row < rows && col < cols && self.state.board[row][col] == Some(player) {
                        count += 1;
                        if count >= win_length {
                            return true;
                        }
                    } else {
                        count = 0;
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
            "rows": self.config.rows,
            "cols": self.config.cols,
            "win_length": self.config.win_length,
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConnectFourResult {
    pub winner: Option<String>,
    pub stats: GameStats,
    pub error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_player_as_str() {
        assert_eq!(Player::Red.as_str(), "Red");
        assert_eq!(Player::Yellow.as_str(), "Yellow");
    }

    #[test]
    fn test_player_to_string() {
        assert_eq!(Player::Red.to_string(), "Red");
        assert_eq!(Player::Yellow.to_string(), "Yellow");
    }

    #[test]
    fn test_player_other() {
        assert_eq!(Player::Red.other(), Player::Yellow);
        assert_eq!(Player::Yellow.other(), Player::Red);
    }

    #[test]
    fn test_connect_four_new() {
        let config = ConnectFourConfig::default();
        let game = ConnectFour::new(config);
        
        assert_eq!(game.state.board.len(), 6);
        assert_eq!(game.state.board[0].len(), 7);
        assert_eq!(game.state.current_player, Player::Red);
        assert_eq!(game.state.turn_number, 0);
        assert_eq!(game.state.game_over, false);
        assert_eq!(game.state.winner, None);
    }

    #[test]
    fn test_connect_four_new_custom_size() {
        let config = ConnectFourConfig {
            rows: 8,
            cols: 10,
            win_length: 5,
        };
        let game = ConnectFour::new(config);
        
        assert_eq!(game.state.board.len(), 8);
        assert_eq!(game.state.board[0].len(), 10);
    }

    #[test]
    fn test_is_valid_move_empty_column() {
        let config = ConnectFourConfig::default();
        let game = ConnectFour::new(config);
        
        assert!(game.is_valid_move(0));
        assert!(game.is_valid_move(3));
        assert!(game.is_valid_move(6));
    }

    #[test]
    fn test_is_valid_move_out_of_bounds() {
        let config = ConnectFourConfig::default();
        let game = ConnectFour::new(config);
        
        assert!(!game.is_valid_move(7));
        assert!(!game.is_valid_move(10));
    }

    #[test]
    fn test_is_valid_move_full_column() {
        let config = ConnectFourConfig::default();
        let mut game = ConnectFour::new(config);
        
        // Fill a column
        for row in 0..6 {
            game.state.board[row][3] = Some(Player::Red);
        }
        
        assert!(!game.is_valid_move(3));
        assert!(game.is_valid_move(0));
    }

    #[test]
    fn test_drop_piece() {
        let config = ConnectFourConfig::default();
        let mut game = ConnectFour::new(config);
        
        // Drop pieces in column 2
        game.drop_piece(2, Player::Red);
        assert_eq!(game.state.board[5][2], Some(Player::Red));
        
        game.drop_piece(2, Player::Yellow);
        assert_eq!(game.state.board[4][2], Some(Player::Yellow));
        
        game.drop_piece(2, Player::Red);
        assert_eq!(game.state.board[3][2], Some(Player::Red));
    }

    #[test]
    fn test_check_win_horizontal() {
        let config = ConnectFourConfig::default();
        let mut game = ConnectFour::new(config);
        
        // Create horizontal win for Red
        game.state.board[5][0] = Some(Player::Red);
        game.state.board[5][1] = Some(Player::Red);
        game.state.board[5][2] = Some(Player::Red);
        game.state.board[5][3] = Some(Player::Red);
        game.state.current_player = Player::Red;
        
        assert!(game.check_win());
    }

    #[test]
    fn test_check_win_vertical() {
        let config = ConnectFourConfig::default();
        let mut game = ConnectFour::new(config);
        
        // Create vertical win for Yellow
        game.state.board[2][3] = Some(Player::Yellow);
        game.state.board[3][3] = Some(Player::Yellow);
        game.state.board[4][3] = Some(Player::Yellow);
        game.state.board[5][3] = Some(Player::Yellow);
        game.state.current_player = Player::Yellow;
        
        assert!(game.check_win());
    }

    #[test]
    fn test_check_win_diagonal_tl_br() {
        let config = ConnectFourConfig::default();
        let mut game = ConnectFour::new(config);
        
        // Create diagonal win (top-left to bottom-right)
        game.state.board[2][0] = Some(Player::Red);
        game.state.board[3][1] = Some(Player::Red);
        game.state.board[4][2] = Some(Player::Red);
        game.state.board[5][3] = Some(Player::Red);
        game.state.current_player = Player::Red;
        
        assert!(game.check_win());
    }

    #[test]
    fn test_check_win_diagonal_tr_bl() {
        let config = ConnectFourConfig::default();
        let mut game = ConnectFour::new(config);
        
        // Create diagonal win (top-right to bottom-left)
        game.state.board[2][3] = Some(Player::Yellow);
        game.state.board[3][2] = Some(Player::Yellow);
        game.state.board[4][1] = Some(Player::Yellow);
        game.state.board[5][0] = Some(Player::Yellow);
        game.state.current_player = Player::Yellow;
        
        assert!(game.check_win());
    }

    #[test]
    fn test_check_win_no_win() {
        let config = ConnectFourConfig::default();
        let mut game = ConnectFour::new(config);
        
        // Partial game, no win
        game.state.board[5][0] = Some(Player::Red);
        game.state.board[5][1] = Some(Player::Yellow);
        game.state.board[4][0] = Some(Player::Red);
        game.state.current_player = Player::Red;
        
        assert!(!game.check_win());
    }

    #[test]
    fn test_check_win_custom_win_length() {
        let config = ConnectFourConfig {
            rows: 6,
            cols: 7,
            win_length: 5,
        };
        let mut game = ConnectFour::new(config);
        
        // Create horizontal win of length 5
        game.state.board[5][0] = Some(Player::Red);
        game.state.board[5][1] = Some(Player::Red);
        game.state.board[5][2] = Some(Player::Red);
        game.state.board[5][3] = Some(Player::Red);
        game.state.board[5][4] = Some(Player::Red);
        game.state.current_player = Player::Red;
        
        assert!(game.check_win());
    }

    #[test]
    fn test_state_to_json() {
        let config = ConnectFourConfig::default();
        let mut game = ConnectFour::new(config);
        game.state.board[5][0] = Some(Player::Red);
        game.state.turn_number = 1;
        
        let json = game.state_to_json();
        assert_eq!(json["board"][5][0], "Red");
        assert_eq!(json["turn_number"], 1);
        assert_eq!(json["rows"], 6);
        assert_eq!(json["cols"], 7);
        assert_eq!(json["win_length"], 4);
    }

    #[test]
    fn test_config_default() {
        let config = ConnectFourConfig::default();
        assert_eq!(config.rows, 6);
        assert_eq!(config.cols, 7);
        assert_eq!(config.win_length, 4);
    }
}

