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

#[derive(Clone, Debug, Deserialize, Default)]
pub enum PlayerOrder {
    Random,
    Decending,
    Ascending,
    #[default]
    OrderInList,
    ReverseOrderInList,
}

#[derive(Clone, Debug, Deserialize)]
pub struct TicTacToeConfig {
    pub board_size: u32,
    pub win_length: u32,
    #[serde(default)]
    #[allow(dead_code)]
    pub order: PlayerOrder,
}

impl Default for TicTacToeConfig {
    fn default() -> Self {
        TicTacToeConfig {
            board_size: 3,
            win_length: 3,
            order: PlayerOrder::default(),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct RockPaperScissorsConfig {
    pub rounds: u32,
    #[serde(default)]
    #[allow(dead_code)]
    pub order: PlayerOrder,
}

impl Default for RockPaperScissorsConfig {
    fn default() -> Self {
        RockPaperScissorsConfig {
            rounds: 3,
            order: PlayerOrder::default(),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct ConnectFourConfig {
    pub rows: u32,
    pub cols: u32,
    pub win_length: u32,
    #[serde(default)]
    #[allow(dead_code)]
    pub order: PlayerOrder,
}

impl Default for ConnectFourConfig {
    fn default() -> Self {
        ConnectFourConfig {
            rows: 6,
            cols: 7,
            win_length: 4,
            order: PlayerOrder::default(),
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

impl From<&str> for Game {
    fn from(name: &str) -> Self {
        match name {
            "TicTacToe" => Game::TicTacToe(TicTacToeConfig::default()),
            "RockPaperScissors" => Game::RockPaperScissors(RockPaperScissorsConfig::default()),
            "ConnectFour" => Game::ConnectFour(ConnectFourConfig::default()),
            _ => panic!("Unknown game name: {}", name),
        }
    }
}

impl Game {
    #[allow(dead_code)]
    pub fn new(name: &str) -> Option<Self> {
        match name {
            "TicTacToe" => Some(Game::TicTacToe(TicTacToeConfig::default())),
            "RockPaperScissors" => {
                Some(Game::RockPaperScissors(RockPaperScissorsConfig::default()))
            }
            "ConnectFour" => Some(Game::ConnectFour(ConnectFourConfig::default())),
            _ => None,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Game::TicTacToe(_) => "TicTacToe",
            Game::RockPaperScissors(_) => "RockPaperScissors",
            Game::ConnectFour(_) => "ConnectFour",
        }
    }

    pub async fn play_game(&self, agents: Vec<AIAgentConfig>) -> TestResult {
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
        assert!(matches!(Game::from("TicTacToe"), Game::TicTacToe(_)));
        assert!(matches!(
            Game::from("RockPaperScissors"),
            Game::RockPaperScissors(_)
        ));
        assert!(matches!(Game::from("ConnectFour"), Game::ConnectFour(_)));
    }

    #[test]
    #[should_panic(expected = "Unknown game name")]
    fn test_game_from_invalid_string() {
        let _ = Game::from("InvalidGame");
    }

    #[test]
    fn test_game_new() {
        assert!(matches!(Game::new("TicTacToe"), Some(Game::TicTacToe(_))));
        assert!(matches!(
            Game::new("RockPaperScissors"),
            Some(Game::RockPaperScissors(_))
        ));
        assert!(matches!(
            Game::new("ConnectFour"),
            Some(Game::ConnectFour(_))
        ));
        assert!(Game::new("InvalidGame").is_none());
    }

    #[test]
    fn test_game_name() {
        assert_eq!(Game::from("TicTacToe").name(), "TicTacToe");
        assert_eq!(Game::from("RockPaperScissors").name(), "RockPaperScissors");
        assert_eq!(Game::from("ConnectFour").name(), "ConnectFour");
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
    fn test_player_order_default() {
        let order = PlayerOrder::default();
        assert!(matches!(order, PlayerOrder::OrderInList));
    }
}
