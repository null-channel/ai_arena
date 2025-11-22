use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Instant;

use crate::agent::{AIAgent, MoveRequest, MoveResponse};
use crate::games::stats::{GameStats, TurnStats};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RockPaperScissorsConfig {
    pub rounds: u32,
}

impl Default for RockPaperScissorsConfig {
    fn default() -> Self {
        RockPaperScissorsConfig { rounds: 3 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Choice {
    Rock,
    Paper,
    Scissors,
}

impl Choice {
    fn as_str(&self) -> &str {
        match self {
            Choice::Rock => "rock",
            Choice::Paper => "paper",
            Choice::Scissors => "scissors",
        }
    }

    fn to_string(&self) -> String {
        self.as_str().to_string()
    }

    fn beats(&self, other: Choice) -> bool {
        matches!(
            (self, other),
            (Choice::Rock, Choice::Scissors)
                | (Choice::Paper, Choice::Rock)
                | (Choice::Scissors, Choice::Paper)
        )
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RockPaperScissorsState {
    pub round: u32,
    pub player_one_score: u32,
    pub player_two_score: u32,
    pub round_history: Vec<RoundResult>,
    pub game_over: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RoundResult {
    pub round_number: u32,
    pub player_one_choice: Option<Choice>,
    pub player_two_choice: Option<Choice>,
    pub winner: Option<usize>, // 0 for player 1, 1 for player 2, None for tie
}

pub struct RockPaperScissors {
    config: RockPaperScissorsConfig,
    state: RockPaperScissorsState,
    stats: GameStats,
    game_id: String,
}

impl RockPaperScissors {
    pub fn new(config: RockPaperScissorsConfig) -> Self {
        Self {
            config,
            state: RockPaperScissorsState {
                round: 0,
                player_one_score: 0,
                player_two_score: 0,
                round_history: Vec::new(),
                game_over: false,
            },
            stats: GameStats::new(),
            game_id: format!("rps_{}", uuid::Uuid::new_v4().to_string()[..8].to_string()),
        }
    }

    pub async fn play_game(mut self, agents: Vec<AIAgent>) -> RockPaperScissorsResult {
        let start_time = Instant::now();

        // Ensure we have exactly 2 agents
        if agents.len() != 2 {
            return RockPaperScissorsResult {
                winner: None,
                stats: self.stats,
                error: Some(format!("Expected 2 agents, got {}", agents.len())),
            };
        }

        let player_one_agent = &agents[0];
        let player_two_agent = &agents[1];

        // Play rounds until someone wins or we run out of rounds
        let rounds_to_win = (self.config.rounds / 2) + 1;
        
        while !self.state.game_over && self.state.round < self.config.rounds {
            self.state.round += 1;

            // Execute round - both players choose simultaneously
            let round_result = match self.execute_round(player_one_agent, player_two_agent).await {
                Ok(result) => result,
                Err(e) => {
                    eprintln!("Round error: {}", e);
                    // Continue with a tie if there's an error
                    RoundResult {
                        round_number: self.state.round,
                        player_one_choice: None,
                        player_two_choice: None,
                        winner: None,
                    }
                }
            };

            // Update scores
            match round_result.winner {
                Some(0) => self.state.player_one_score += 1,
                Some(1) => self.state.player_two_score += 1,
                _ => {} // Tie, no score change
            }

            self.state.round_history.push(round_result.clone());

            // Check for game end
            if self.state.player_one_score >= rounds_to_win {
                self.state.game_over = true;
                self.stats.winner = Some(format!("{} (Player 1)", player_one_agent.name()));
                break;
            } else if self.state.player_two_score >= rounds_to_win {
                self.state.game_over = true;
                self.stats.winner = Some(format!("{} (Player 2)", player_two_agent.name()));
                break;
            }
        }

        // If game ended without a clear winner (all rounds played, tie)
        if !self.state.game_over {
            if self.state.player_one_score > self.state.player_two_score {
                self.stats.winner = Some(format!("{} (Player 1)", player_one_agent.name()));
            } else if self.state.player_two_score > self.state.player_one_score {
                self.stats.winner = Some(format!("{} (Player 2)", player_two_agent.name()));
            } else {
                self.stats.draw = true;
            }
            self.state.game_over = true;
        }

        let total_duration = start_time.elapsed();
        self.stats.total_duration_ms = total_duration.as_millis() as u64;

        RockPaperScissorsResult {
            winner: self.stats.winner.clone(),
            stats: self.stats,
            error: None,
        }
    }

    async fn execute_round(
        &mut self,
        player_one_agent: &AIAgent,
        player_two_agent: &AIAgent,
    ) -> Result<RoundResult, String> {
        let round_start = Instant::now();

        // Create game state JSON
        let state_json = self.state_to_json();
        let state_before = state_json.clone();

        // Create move schema
        let move_schema = json!({
            "type": "object",
            "properties": {
                "choice": {
                    "type": "string",
                    "enum": ["rock", "paper", "scissors"],
                    "description": "Your choice for this round"
                }
            },
            "required": ["choice"]
        });

        // Both players choose simultaneously
        let turn_number = self.state.round;

        let move_request_one = MoveRequest {
            turn_index: turn_number,
            game_id: self.game_id.clone(),
            state: state_json.clone(),
            expected_move_schema: move_schema.clone(),
        };

        let move_request_two = MoveRequest {
            turn_index: turn_number,
            game_id: self.game_id.clone(),
            state: state_json.clone(),
            expected_move_schema: move_schema.clone(),
        };

        // Get moves from both agents (could be parallelized in the future)
        let move_response_one: MoveResponse = player_one_agent
            .execute_turn(&move_request_one)
            .await
            .map_err(|e| format!("Player 1 error: {}", e))?;

        let move_response_two: MoveResponse = player_two_agent
            .execute_turn(&move_request_two)
            .await
            .map_err(|e| format!("Player 2 error: {}", e))?;

        let time_taken = round_start.elapsed();

        // Parse choices
        let choice_one_result = self.parse_choice(&move_response_one.chosen_move, "Player 1");
        let choice_two_result = self.parse_choice(&move_response_two.chosen_move, "Player 2");
        
        let (choice_one, choice_one_valid, choice_one_error) = match choice_one_result {
            Ok(Some(c)) => (Some(c), true, None),
            Ok(None) => (None, false, Some("Invalid choice".to_string())),
            Err(e) => (None, false, Some(e)),
        };
        
        let (choice_two, choice_two_valid, choice_two_error) = match choice_two_result {
            Ok(Some(c)) => (Some(c), true, None),
            Ok(None) => (None, false, Some("Invalid choice".to_string())),
            Err(e) => (None, false, Some(e)),
        };

        // Determine winner (only if both choices are valid)
        let winner = if let (Some(c1), Some(c2)) = (choice_one, choice_two) {
            if c1.beats(c2) {
                Some(0) // Player 1 wins
            } else if c2.beats(c1) {
                Some(1) // Player 2 wins
            } else {
                None // Tie
            }
        } else {
            None // Invalid moves result in no winner
        };

        // Record turn stats for player 1
        let turn_stats_one = TurnStats {
            turn_number: turn_number * 2 - 1, // Odd numbers for player 1
            player: player_one_agent.name().to_string(),
            move_made: move_response_one.chosen_move.clone(),
            time_taken_ms: time_taken.as_millis() as u64,
            move_valid: choice_one_valid,
            error_message: choice_one_error,
            state_before: state_before.clone(),
            state_after: self.state_to_json(),
            diagnostics: move_response_one.diagnostics,
        };
        self.stats.add_turn(turn_stats_one);

        // Record turn stats for player 2
        let turn_stats_two = TurnStats {
            turn_number: turn_number * 2, // Even numbers for player 2
            player: player_two_agent.name().to_string(),
            move_made: move_response_two.chosen_move.clone(),
            time_taken_ms: time_taken.as_millis() as u64,
            move_valid: choice_two_valid,
            error_message: choice_two_error,
            state_before: state_before.clone(),
            state_after: self.state_to_json(),
            diagnostics: move_response_two.diagnostics,
        };
        self.stats.add_turn(turn_stats_two);

        Ok(RoundResult {
            round_number: turn_number,
            player_one_choice: choice_one,
            player_two_choice: choice_two,
            winner,
        })
    }

    fn parse_choice(&self, move_data: &Value, player_name: &str) -> Result<Option<Choice>, String> {
        let choice_str = move_data
            .get("choice")
            .and_then(|v| v.as_str())
            .ok_or_else(|| format!("{}: Missing or invalid 'choice' field", player_name))?;

        match choice_str.to_lowercase().as_str() {
            "rock" => Ok(Some(Choice::Rock)),
            "paper" => Ok(Some(Choice::Paper)),
            "scissors" => Ok(Some(Choice::Scissors)),
            _ => Ok(None), // Invalid choice, but don't fail the round
        }
    }

    fn state_to_json(&self) -> Value {
        let round_history: Vec<Value> = self
            .state
            .round_history
            .iter()
            .map(|r| {
                json!({
                    "round_number": r.round_number,
                    "player_one_choice": r.player_one_choice.map(|c| c.to_string()),
                    "player_two_choice": r.player_two_choice.map(|c| c.to_string()),
                    "winner": r.winner,
                })
            })
            .collect();

        json!({
            "round": self.state.round,
            "player_one_score": self.state.player_one_score,
            "player_two_score": self.state.player_two_score,
            "round_history": round_history,
            "game_over": self.state.game_over,
            "total_rounds": self.config.rounds,
            "rounds_to_win": (self.config.rounds / 2) + 1,
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RockPaperScissorsResult {
    pub winner: Option<String>,
    pub stats: GameStats,
    pub error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_choice_as_str() {
        assert_eq!(Choice::Rock.as_str(), "rock");
        assert_eq!(Choice::Paper.as_str(), "paper");
        assert_eq!(Choice::Scissors.as_str(), "scissors");
    }

    #[test]
    fn test_choice_to_string() {
        assert_eq!(Choice::Rock.to_string(), "rock");
        assert_eq!(Choice::Paper.to_string(), "paper");
        assert_eq!(Choice::Scissors.to_string(), "scissors");
    }

    #[test]
    fn test_choice_beats() {
        // Rock beats Scissors
        assert!(Choice::Rock.beats(Choice::Scissors));
        assert!(!Choice::Scissors.beats(Choice::Rock));
        
        // Paper beats Rock
        assert!(Choice::Paper.beats(Choice::Rock));
        assert!(!Choice::Rock.beats(Choice::Paper));
        
        // Scissors beats Paper
        assert!(Choice::Scissors.beats(Choice::Paper));
        assert!(!Choice::Paper.beats(Choice::Scissors));
        
        // Same choices don't beat each other
        assert!(!Choice::Rock.beats(Choice::Rock));
        assert!(!Choice::Paper.beats(Choice::Paper));
        assert!(!Choice::Scissors.beats(Choice::Scissors));
    }

    #[test]
    fn test_rock_paper_scissors_new() {
        let config = RockPaperScissorsConfig::default();
        let game = RockPaperScissors::new(config);
        
        assert_eq!(game.state.round, 0);
        assert_eq!(game.state.player_one_score, 0);
        assert_eq!(game.state.player_two_score, 0);
        assert_eq!(game.state.round_history.len(), 0);
        assert_eq!(game.state.game_over, false);
    }

    #[test]
    fn test_rock_paper_scissors_new_custom_rounds() {
        let config = RockPaperScissorsConfig { rounds: 5 };
        let game = RockPaperScissors::new(config);
        
        assert_eq!(game.config.rounds, 5);
    }

    #[test]
    fn test_parse_choice_valid() {
        let config = RockPaperScissorsConfig::default();
        let game = RockPaperScissors::new(config);
        
        use serde_json::json;
        
        assert_eq!(
            game.parse_choice(&json!({"choice": "rock"}), "Test").unwrap(),
            Some(Choice::Rock)
        );
        assert_eq!(
            game.parse_choice(&json!({"choice": "paper"}), "Test").unwrap(),
            Some(Choice::Paper)
        );
        assert_eq!(
            game.parse_choice(&json!({"choice": "scissors"}), "Test").unwrap(),
            Some(Choice::Scissors)
        );
        
        // Case insensitive
        assert_eq!(
            game.parse_choice(&json!({"choice": "ROCK"}), "Test").unwrap(),
            Some(Choice::Rock)
        );
        assert_eq!(
            game.parse_choice(&json!({"choice": "Paper"}), "Test").unwrap(),
            Some(Choice::Paper)
        );
    }

    #[test]
    fn test_parse_choice_invalid() {
        let config = RockPaperScissorsConfig::default();
        let game = RockPaperScissors::new(config);
        
        use serde_json::json;
        
        // Invalid choice string
        assert_eq!(
            game.parse_choice(&json!({"choice": "invalid"}), "Test").unwrap(),
            None
        );
        
        // Missing choice field
        assert!(game.parse_choice(&json!({}), "Test").is_err());
        
        // Wrong type
        assert!(game.parse_choice(&json!({"choice": 123}), "Test").is_err());
    }

    #[test]
    fn test_config_default() {
        let config = RockPaperScissorsConfig::default();
        assert_eq!(config.rounds, 3);
    }
}

