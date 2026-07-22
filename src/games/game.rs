use serde::{Deserialize, Serialize};

use crate::agent_config::{AIAgentConfig, build_agents};

use super::connect_four::{ConnectFour, ConnectFourConfig as GameConnectFourConfig};
use super::rock_paper_scissors::{
    RockPaperScissors, RockPaperScissorsConfig as GameRockPaperScissorsConfig,
};
use super::stats::{GameOutcome, GameStats};
use super::tic_tac_toe::{TicTacToe, TicTacToeConfig as GameTicTacToeConfig};

#[derive(Clone, Debug, Deserialize)]
pub enum Game {
    TicTacToe(TicTacToeConfig),
    RockPaperScissors(RockPaperScissorsConfig),
    ConnectFour(ConnectFourConfig),
}

#[derive(Clone, Debug, Deserialize)]
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

#[derive(Clone, Debug, Deserialize)]
pub struct RockPaperScissorsConfig {
    pub rounds: u32,
}

impl Default for RockPaperScissorsConfig {
    fn default() -> Self {
        RockPaperScissorsConfig { rounds: 3 }
    }
}

#[derive(Clone, Debug, Deserialize)]
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
pub enum TestResult {
    TicTacToe(TicTacToeResult),
    RockPaperScissors(RockPaperScissorsResult),
    ConnectFour(ConnectFourResult),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TicTacToeResult {
    pub stats: GameStats,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RockPaperScissorsResult {
    pub stats: GameStats,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConnectFourResult {
    pub stats: GameStats,
}

impl TestResult {
    pub fn stats(&self) -> &GameStats {
        match self {
            Self::TicTacToe(result) => &result.stats,
            Self::RockPaperScissors(result) => &result.stats,
            Self::ConnectFour(result) => &result.stats,
        }
    }
}

impl TryFrom<&str> for Game {
    type Error = String;

    fn try_from(name: &str) -> Result<Self, Self::Error> {
        match name {
            "TicTacToe" => Ok(Game::TicTacToe(TicTacToeConfig::default())),
            "RockPaperScissors" => Ok(Game::RockPaperScissors(RockPaperScissorsConfig::default())),
            "ConnectFour" => Ok(Game::ConnectFour(ConnectFourConfig::default())),
            _ => Err(format!(
                "unknown game '{name}'; expected TicTacToe, RockPaperScissors, or ConnectFour"
            )),
        }
    }
}

impl Game {
    pub fn name(&self) -> &str {
        match self {
            Game::TicTacToe(_) => "TicTacToe",
            Game::RockPaperScissors(_) => "RockPaperScissors",
            Game::ConnectFour(_) => "ConnectFour",
        }
    }

    pub async fn play_game(&self, agents: Vec<AIAgentConfig>) -> TestResult {
        if let Err(error) = self.validate() {
            return self.error_result(error);
        }
        let agents = match build_agents(agents) {
            Ok(agents) => agents,
            Err(error) => return self.error_result(error.to_string()),
        };

        match self {
            Game::TicTacToe(config) => {
                let game_config = GameTicTacToeConfig {
                    board_size: config.board_size,
                    win_length: config.win_length,
                };
                let game = TicTacToe::new(game_config);
                let result = game.play_game(agents).await;

                TestResult::TicTacToe(TicTacToeResult {
                    stats: result.stats,
                })
            }
            Game::RockPaperScissors(config) => {
                let game_config = GameRockPaperScissorsConfig {
                    rounds: config.rounds,
                };
                let game = RockPaperScissors::new(game_config);
                let result = game.play_game(agents).await;

                TestResult::RockPaperScissors(RockPaperScissorsResult {
                    stats: result.stats,
                })
            }
            Game::ConnectFour(config) => {
                let game_config = GameConnectFourConfig {
                    rows: config.rows,
                    cols: config.cols,
                    win_length: config.win_length,
                };
                let game = ConnectFour::new(game_config);
                let result = game.play_game(agents).await;

                TestResult::ConnectFour(ConnectFourResult {
                    stats: result.stats,
                })
            }
        }
    }

    fn validate(&self) -> Result<(), String> {
        match self {
            Game::TicTacToe(config) => {
                if config.board_size == 0 {
                    return Err("TicTacToe board_size must be greater than zero".into());
                }
                if config.win_length == 0 || config.win_length > config.board_size {
                    return Err("TicTacToe win_length must be between 1 and board_size".into());
                }
            }
            Game::RockPaperScissors(config) if config.rounds == 0 => {
                return Err("RockPaperScissors rounds must be greater than zero".into());
            }
            Game::ConnectFour(config) => {
                if config.rows == 0 || config.cols == 0 {
                    return Err("ConnectFour rows and cols must be greater than zero".into());
                }
                if config.win_length == 0
                    || config.win_length > std::cmp::max(config.rows, config.cols)
                {
                    return Err(
                        "ConnectFour win_length must fit within at least one board dimension"
                            .into(),
                    );
                }
            }
            Game::RockPaperScissors(_) => {}
        }
        Ok(())
    }

    fn error_result(&self, error: String) -> TestResult {
        let mut stats = GameStats::new();
        stats.outcome = GameOutcome::Error { message: error };
        match self {
            Game::TicTacToe(_) => TestResult::TicTacToe(TicTacToeResult { stats }),
            Game::RockPaperScissors(_) => {
                TestResult::RockPaperScissors(RockPaperScissorsResult { stats })
            }
            Game::ConnectFour(_) => TestResult::ConnectFour(ConnectFourResult { stats }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_game_from_string() {
        assert!(matches!(
            Game::try_from("TicTacToe"),
            Ok(Game::TicTacToe(_))
        ));
        assert!(matches!(
            Game::try_from("RockPaperScissors"),
            Ok(Game::RockPaperScissors(_))
        ));
        assert!(matches!(
            Game::try_from("ConnectFour"),
            Ok(Game::ConnectFour(_))
        ));
    }

    #[test]
    fn test_game_from_invalid_string() {
        assert!(Game::try_from("InvalidGame").is_err());
    }

    #[test]
    fn test_game_name() {
        assert_eq!(Game::try_from("TicTacToe").unwrap().name(), "TicTacToe");
        assert_eq!(
            Game::try_from("RockPaperScissors").unwrap().name(),
            "RockPaperScissors"
        );
        assert_eq!(Game::try_from("ConnectFour").unwrap().name(), "ConnectFour");
    }

    #[test]
    fn test_tic_tac_toe_config_default() {
        let config = TicTacToeConfig::default();
        assert_eq!(config.board_size, 3);
        assert_eq!(config.win_length, 3);
    }

    #[test]
    fn test_rock_paper_scissors_config_default() {
        let config = RockPaperScissorsConfig::default();
        assert_eq!(config.rounds, 3);
    }

    #[test]
    fn test_connect_four_config_default() {
        let config = ConnectFourConfig::default();
        assert_eq!(config.rows, 6);
        assert_eq!(config.cols, 7);
        assert_eq!(config.win_length, 4);
    }

    #[test]
    fn rejects_invalid_game_dimensions() {
        assert!(
            Game::TicTacToe(TicTacToeConfig {
                board_size: 0,
                win_length: 0,
            })
            .validate()
            .is_err()
        );
        assert!(
            Game::ConnectFour(ConnectFourConfig {
                rows: 6,
                cols: 7,
                win_length: 8,
            })
            .validate()
            .is_err()
        );
        assert!(
            Game::RockPaperScissors(RockPaperScissorsConfig { rounds: 0 })
                .validate()
                .is_err()
        );
    }
}
