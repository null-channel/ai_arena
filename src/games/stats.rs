use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Statistics tracked for each turn in a game
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TurnStats {
    /// The turn number (0-indexed)
    pub turn_number: u32,
    /// The player who made this turn
    pub player: String,
    /// The move that was made (as JSON)
    pub move_made: Value,
    /// Time taken to make the move
    pub time_taken_ms: u64,
    /// Whether the move was valid
    pub move_valid: bool,
    /// Error message if move was invalid
    pub error_message: Option<String>,
    /// The game state before this move
    pub state_before: Value,
    /// The game state after this move
    pub state_after: Value,
    /// Any diagnostics from the agent
    pub diagnostics: Option<String>,
}

/// Statistics for a complete game
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GameStats {
    /// All turns in the game
    pub turns: Vec<TurnStats>,
    /// Total game duration in milliseconds
    pub total_duration_ms: u64,
    /// Number of invalid moves attempted
    pub invalid_moves: u32,
    /// Winner of the game (None if draw or incomplete)
    pub winner: Option<String>,
    /// Whether the game ended in a draw
    pub draw: bool,
}

impl GameStats {
    pub fn new() -> Self {
        Self {
            turns: Vec::new(),
            total_duration_ms: 0,
            invalid_moves: 0,
            winner: None,
            draw: false,
        }
    }

    pub fn add_turn(&mut self, turn: TurnStats) {
        if !turn.move_valid {
            self.invalid_moves += 1;
        }
        self.turns.push(turn);
    }

    pub fn average_turn_time_ms(&self) -> f64 {
        if self.turns.is_empty() {
            return 0.0;
        }
        let total: u64 = self.turns.iter().map(|t| t.time_taken_ms).sum();
        total as f64 / self.turns.len() as f64
    }

    pub fn total_turns(&self) -> u32 {
        self.turns.len() as u32
    }
}

impl Default for GameStats {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_game_stats_new() {
        let stats = GameStats::new();
        assert_eq!(stats.turns.len(), 0);
        assert_eq!(stats.total_duration_ms, 0);
        assert_eq!(stats.invalid_moves, 0);
        assert_eq!(stats.winner, None);
        assert_eq!(stats.draw, false);
    }

    #[test]
    fn test_game_stats_default() {
        let stats = GameStats::default();
        assert_eq!(stats.turns.len(), 0);
        assert_eq!(stats.total_duration_ms, 0);
        assert_eq!(stats.invalid_moves, 0);
    }

    #[test]
    fn test_add_valid_turn() {
        let mut stats = GameStats::new();
        let turn = TurnStats {
            turn_number: 1,
            player: "Player1".to_string(),
            move_made: json!({"row": 0, "col": 0}),
            time_taken_ms: 100,
            move_valid: true,
            error_message: None,
            state_before: json!({}),
            state_after: json!({}),
            diagnostics: None,
        };
        
        stats.add_turn(turn);
        assert_eq!(stats.turns.len(), 1);
        assert_eq!(stats.invalid_moves, 0);
    }

    #[test]
    fn test_add_invalid_turn() {
        let mut stats = GameStats::new();
        let turn = TurnStats {
            turn_number: 1,
            player: "Player1".to_string(),
            move_made: json!({"row": 10, "col": 10}),
            time_taken_ms: 50,
            move_valid: false,
            error_message: Some("Invalid move".to_string()),
            state_before: json!({}),
            state_after: json!({}),
            diagnostics: None,
        };
        
        stats.add_turn(turn);
        assert_eq!(stats.turns.len(), 1);
        assert_eq!(stats.invalid_moves, 1);
    }

    #[test]
    fn test_add_multiple_turns() {
        let mut stats = GameStats::new();
        
        for i in 0..5 {
            let turn = TurnStats {
                turn_number: i,
                player: format!("Player{}", i % 2 + 1),
                move_made: json!({"move": i}),
                time_taken_ms: ((i + 1) * 10) as u64,
                move_valid: i % 2 == 0, // Alternate valid/invalid
                error_message: if i % 2 == 0 { None } else { Some("Invalid".to_string()) },
                state_before: json!({}),
                state_after: json!({}),
                diagnostics: None,
            };
            stats.add_turn(turn);
        }
        
        assert_eq!(stats.turns.len(), 5);
        assert_eq!(stats.invalid_moves, 2); // Turns 1 and 3 are invalid
    }

    #[test]
    fn test_average_turn_time_empty() {
        let stats = GameStats::new();
        assert_eq!(stats.average_turn_time_ms(), 0.0);
    }

    #[test]
    fn test_average_turn_time_single() {
        let mut stats = GameStats::new();
        let turn = TurnStats {
            turn_number: 1,
            player: "Player1".to_string(),
            move_made: json!({}),
            time_taken_ms: 100,
            move_valid: true,
            error_message: None,
            state_before: json!({}),
            state_after: json!({}),
            diagnostics: None,
        };
        stats.add_turn(turn);
        assert_eq!(stats.average_turn_time_ms(), 100.0);
    }

    #[test]
    fn test_average_turn_time_multiple() {
        let mut stats = GameStats::new();
        for i in 1..=5 {
            let turn = TurnStats {
                turn_number: i,
                player: "Player1".to_string(),
                move_made: json!({}),
                time_taken_ms: (i * 10) as u64,
                move_valid: true,
                error_message: None,
                state_before: json!({}),
                state_after: json!({}),
                diagnostics: None,
            };
            stats.add_turn(turn);
        }
        // Average of 10, 20, 30, 40, 50 = 30.0
        assert_eq!(stats.average_turn_time_ms(), 30.0);
    }

    #[test]
    fn test_total_turns() {
        let mut stats = GameStats::new();
        assert_eq!(stats.total_turns(), 0);
        
        for i in 0..3 {
            let turn = TurnStats {
                turn_number: i,
                player: "Player1".to_string(),
                move_made: json!({}),
                time_taken_ms: 100,
                move_valid: true,
                error_message: None,
                state_before: json!({}),
                state_after: json!({}),
                diagnostics: None,
            };
            stats.add_turn(turn);
        }
        
        assert_eq!(stats.total_turns(), 3);
    }
}

