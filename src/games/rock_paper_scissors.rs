use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::time::Instant;

use crate::agent::{AgentResult, GameAgent, MoveRequest, MoveResponse};
use crate::games::stats::{GameOutcome, GameStats, TurnStats};

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

struct RoundExecution {
    result: RoundResult,
    failure: Option<RoundFailure>,
}

enum RoundFailure {
    Player { index: usize, reason: String },
    Both { reason: String },
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
            game_id: format!("rps_{}", &uuid::Uuid::new_v4().to_string()[..8]),
        }
    }

    pub async fn play_game<A: GameAgent>(mut self, agents: Vec<A>) -> RockPaperScissorsResult {
        let start_time = Instant::now();

        // Ensure we have exactly 2 agents
        if agents.len() != 2 {
            return RockPaperScissorsResult {
                stats: GameStats {
                    outcome: GameOutcome::Error {
                        message: format!("Expected 2 agents, got {}", agents.len()),
                    },
                    ..self.stats
                },
            };
        }

        let player_one_agent = &agents[0];
        let player_two_agent = &agents[1];

        // Play rounds until someone wins or we run out of rounds
        let rounds_to_win = (self.config.rounds / 2) + 1;

        while !self.state.game_over && self.state.round < self.config.rounds {
            self.state.round += 1;

            // Execute round - both players choose simultaneously
            let execution = self.execute_round(player_one_agent, player_two_agent).await;
            let round_result = execution.result;
            self.state.round_history.push(round_result.clone());

            if let Some(failure) = execution.failure {
                self.state.game_over = true;
                self.stats.outcome = match failure {
                    RoundFailure::Player { index, reason } => {
                        let (winner, loser) = if index == 0 {
                            (player_two_agent.name(), player_one_agent.name())
                        } else {
                            (player_one_agent.name(), player_two_agent.name())
                        };
                        GameOutcome::Forfeit {
                            winner: winner.to_owned(),
                            loser: loser.to_owned(),
                            reason,
                        }
                    }
                    RoundFailure::Both { reason } => GameOutcome::Error { message: reason },
                };
                break;
            }

            // Update scores
            match round_result.winner {
                Some(0) => self.state.player_one_score += 1,
                Some(1) => self.state.player_two_score += 1,
                _ => {} // Tie, no score change
            }

            // Check for game end
            if self.state.player_one_score >= rounds_to_win {
                self.state.game_over = true;
                self.stats.outcome = GameOutcome::Winner {
                    winner: player_one_agent.name().to_owned(),
                };
                break;
            } else if self.state.player_two_score >= rounds_to_win {
                self.state.game_over = true;
                self.stats.outcome = GameOutcome::Winner {
                    winner: player_two_agent.name().to_owned(),
                };
                break;
            }
        }

        // If game ended without a clear winner (all rounds played, tie)
        if !self.state.game_over {
            if self.state.player_one_score > self.state.player_two_score {
                self.stats.outcome = GameOutcome::Winner {
                    winner: player_one_agent.name().to_owned(),
                };
            } else if self.state.player_two_score > self.state.player_one_score {
                self.stats.outcome = GameOutcome::Winner {
                    winner: player_two_agent.name().to_owned(),
                };
            } else {
                self.stats.outcome = GameOutcome::Draw;
            }
            self.state.game_over = true;
        }

        let total_duration = start_time.elapsed();
        self.stats.total_duration_ms = total_duration.as_millis() as u64;

        RockPaperScissorsResult { stats: self.stats }
    }

    async fn execute_round(
        &mut self,
        player_one_agent: &impl GameAgent,
        player_two_agent: &impl GameAgent,
    ) -> RoundExecution {
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

        let player_one_future = async {
            let start = Instant::now();
            let response = player_one_agent.execute_turn(&move_request_one).await;
            (response, start.elapsed())
        };
        let player_two_future = async {
            let start = Instant::now();
            let response = player_two_agent.execute_turn(&move_request_two).await;
            (response, start.elapsed())
        };
        let ((response_one, duration_one), (response_two, duration_two)) =
            tokio::join!(player_one_future, player_two_future);

        let (move_one, diagnostics_one, usage_one, choice_one, choice_one_error) =
            self.evaluate_response(response_one, "Player 1");
        let (move_two, diagnostics_two, usage_two, choice_two, choice_two_error) =
            self.evaluate_response(response_two, "Player 2");
        let choice_one_valid = choice_one_error.is_none();
        let choice_two_valid = choice_two_error.is_none();

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
            move_made: move_one,
            time_taken_ms: duration_one.as_millis() as u64,
            move_valid: choice_one_valid,
            error_message: choice_one_error.clone(),
            state_before: state_before.clone(),
            state_after: self.state_to_json(),
            diagnostics: diagnostics_one,
            token_usage: usage_one,
        };
        self.stats.add_turn(turn_stats_one);

        // Record turn stats for player 2
        let turn_stats_two = TurnStats {
            turn_number: turn_number * 2, // Even numbers for player 2
            player: player_two_agent.name().to_string(),
            move_made: move_two,
            time_taken_ms: duration_two.as_millis() as u64,
            move_valid: choice_two_valid,
            error_message: choice_two_error.clone(),
            state_before: state_before.clone(),
            state_after: self.state_to_json(),
            diagnostics: diagnostics_two,
            token_usage: usage_two,
        };
        self.stats.add_turn(turn_stats_two);

        let failure = match (choice_one_error, choice_two_error) {
            (Some(one), Some(two)) => Some(RoundFailure::Both {
                reason: format!("both agents failed: {one}; {two}"),
            }),
            (Some(reason), None) => Some(RoundFailure::Player { index: 0, reason }),
            (None, Some(reason)) => Some(RoundFailure::Player { index: 1, reason }),
            (None, None) => None,
        };

        RoundExecution {
            result: RoundResult {
                round_number: turn_number,
                player_one_choice: choice_one,
                player_two_choice: choice_two,
                winner,
            },
            failure,
        }
    }

    fn evaluate_response(
        &self,
        response: AgentResult<MoveResponse>,
        player_name: &str,
    ) -> (
        Value,
        Option<String>,
        Option<crate::agent::TokenUsage>,
        Option<Choice>,
        Option<String>,
    ) {
        match response {
            Ok(response) => {
                let choice = self.parse_choice(&response.chosen_move, player_name);
                match choice {
                    Ok(Some(choice)) => (
                        response.chosen_move,
                        response.diagnostics,
                        response.token_usage,
                        Some(choice),
                        None,
                    ),
                    Ok(None) => (
                        response.chosen_move,
                        response.diagnostics,
                        response.token_usage,
                        None,
                        Some(format!("{player_name}: invalid choice")),
                    ),
                    Err(error) => (
                        response.chosen_move,
                        response.diagnostics,
                        response.token_usage,
                        None,
                        Some(error),
                    ),
                }
            }
            Err(error) => {
                let token_usage = error.token_usage();
                (
                    Value::Null,
                    None,
                    token_usage,
                    None,
                    Some(format!("{player_name}: agent error: {error}")),
                )
            }
        }
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
                    "player_one_choice": r.player_one_choice.map(|c| c.as_str().to_owned()),
                    "player_two_choice": r.player_two_choice.map(|c| c.as_str().to_owned()),
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
    pub stats: GameStats,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::ScriptedAgent;

    #[test]
    fn test_choice_as_str() {
        assert_eq!(Choice::Rock.as_str(), "rock");
        assert_eq!(Choice::Paper.as_str(), "paper");
        assert_eq!(Choice::Scissors.as_str(), "scissors");
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
        assert!(!game.state.game_over);
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
            game.parse_choice(&json!({"choice": "rock"}), "Test")
                .unwrap(),
            Some(Choice::Rock)
        );
        assert_eq!(
            game.parse_choice(&json!({"choice": "paper"}), "Test")
                .unwrap(),
            Some(Choice::Paper)
        );
        assert_eq!(
            game.parse_choice(&json!({"choice": "scissors"}), "Test")
                .unwrap(),
            Some(Choice::Scissors)
        );

        // Case insensitive
        assert_eq!(
            game.parse_choice(&json!({"choice": "ROCK"}), "Test")
                .unwrap(),
            Some(Choice::Rock)
        );
        assert_eq!(
            game.parse_choice(&json!({"choice": "Paper"}), "Test")
                .unwrap(),
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
            game.parse_choice(&json!({"choice": "invalid"}), "Test")
                .unwrap(),
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

    #[tokio::test]
    async fn invalid_choice_is_recorded_as_a_forfeit() {
        let player_one = ScriptedAgent::new("one", vec![json!({"choice": "lizard"})]);
        let player_two = ScriptedAgent::new("two", vec![json!({"choice": "rock"})]);

        let result = RockPaperScissors::new(RockPaperScissorsConfig::default())
            .play_game(vec![player_one, player_two])
            .await;

        assert_eq!(result.stats.total_turns(), 2);
        assert_eq!(result.stats.invalid_moves, 1);
        assert!(matches!(
            result.stats.outcome,
            GameOutcome::Forfeit {
                ref winner,
                ref loser,
                ..
            } if winner == "two" && loser == "one"
        ));
    }
}
