use serde::{Deserialize, Serialize};

use crate::agent_config::{AIAgentConfig, build_agents};

use super::connect_four::{ConnectFour, ConnectFourConfig as GameConnectFourConfig};
use super::rock_paper_scissors::{
    RockPaperScissors, RockPaperScissorsConfig as GameRockPaperScissorsConfig,
};
use super::stats::GameStats;
use super::tic_tac_toe::{TicTacToe, TicTacToeConfig as GameTicTacToeConfig};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum Game {
    TicTacToe(TicTacToeConfig),
    RockPaperScissors(RockPaperScissorsConfig),
    ConnectFour(ConnectFourConfig),
    Chess(ChessConfig),
    Checkers(CheckersConfig),
    Battleship(BattleshipConfig),
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub enum PlayerOrder {
    Random,
    Decending,
    Ascending,
    #[default]
    OrderInList,
    ReverseOrderInList,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
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

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
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

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
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

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ChessConfig {
    #[serde(default)]
    pub start_fen: Option<String>,
    #[serde(default)]
    pub order: PlayerOrder,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CheckersConfig {
    pub max_turns: u32,
    #[serde(default)]
    pub order: PlayerOrder,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct BattleshipConfig {
    pub max_turns: u32,
    #[serde(default)]
    pub order: PlayerOrder,
}

impl Default for BattleshipConfig {
    fn default() -> Self {
        Self {
            max_turns: 160,
            order: PlayerOrder::default(),
        }
    }
}

impl Default for CheckersConfig {
    fn default() -> Self {
        Self {
            max_turns: 200,
            order: PlayerOrder::default(),
        }
    }
}

impl Default for ChessConfig {
    fn default() -> Self {
        ChessConfig {
            start_fen: None,
            order: PlayerOrder::default(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum TestResult {
    TicTacToe(TicTacToeResult),
    RockPaperScissors(RockPaperScissorsResult),
    ConnectFour(ConnectFourResult),
    Chess(ChessResult),
    Checkers(CheckersResult),
    Battleship(BattleshipResult),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TicTacToeResult {
    pub winner: Option<String>,
    pub stats: GameStats,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RockPaperScissorsResult {
    pub winner: Option<String>,
    pub stats: GameStats,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConnectFourResult {
    pub winner: Option<String>,
    pub stats: GameStats,
    pub error: Option<String>,
}

impl From<&str> for Game {
    fn from(name: &str) -> Self {
        match name.to_lowercase().as_str() {
            "tictactoe" => Game::TicTacToe(TicTacToeConfig::default()),
            "rockpaperscissors" => Game::RockPaperScissors(RockPaperScissorsConfig::default()),
            "connectfour" => Game::ConnectFour(ConnectFourConfig::default()),
            "chess" => Game::Chess(ChessConfig::default()),
            "checkers" => Game::Checkers(CheckersConfig::default()),
            "battleship" => Game::Battleship(BattleshipConfig::default()),
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
            Game::Chess(_) => "Chess",
            Game::Checkers(_) => "Checkers",
            Game::Battleship(_) => "Battleship",
        }
    }

    pub async fn play_game(
        &self,
        agents: Vec<AIAgentConfig>,
        observer: Option<tokio::sync::mpsc::Sender<GameEvent>>,
        secrets: Option<&crate::secrets::SecretsManager>,
    ) -> TestResult {
        let result = match self {
            Game::TicTacToe(config) => {
                let agents = build_agents(agents, secrets);
                let game_config = GameTicTacToeConfig {
                    board_size: config.board_size,
                    win_length: config.win_length,
                };
                let game = TicTacToe::new(game_config);
                let result = game.play_game(agents).await;

                TestResult::TicTacToe(TicTacToeResult {
                    winner: result.winner.clone(),
                    stats: result.stats,
                    error: result.error,
                })
            }
            Game::RockPaperScissors(config) => {
                let agents = build_agents(agents, secrets);
                let game_config = GameRockPaperScissorsConfig {
                    rounds: config.rounds,
                };
                let game = RockPaperScissors::new(game_config);
                let result = game.play_game(agents).await;

                TestResult::RockPaperScissors(RockPaperScissorsResult {
                    winner: result.winner.clone(),
                    stats: result.stats,
                    error: result.error,
                })
            }
            Game::ConnectFour(config) => {
                let agents = build_agents(agents, secrets);
                let game_config = GameConnectFourConfig {
                    rows: config.rows,
                    cols: config.cols,
                    win_length: config.win_length,
                };
                let game = ConnectFour::new(game_config);
                let result = game.play_game(agents).await;

                TestResult::ConnectFour(ConnectFourResult {
                    winner: result.winner.clone(),
                    stats: result.stats,
                    error: result.error,
                })
            }
            Game::Chess(config) => {
                let agents = build_agents(agents, secrets);
                let game_config = GameChessConfig {
                    start_fen: config.start_fen.clone(),
                };
                let game = Chess::new(game_config);
                let result = game.play_game(agents, observer.clone()).await;

                TestResult::Chess(result)
            }
            Game::Checkers(config) => {
                let agents = build_agents(agents, secrets);
                let game = Checkers::new(GameCheckersConfig {
                    max_turns: config.max_turns,
                });
                TestResult::Checkers(game.play_game(agents, observer.clone()).await)
            }
            Game::Battleship(config) => {
                let agents = build_agents(agents, secrets);
                let game = Battleship::new(GameBattleshipConfig {
                    max_turns: config.max_turns,
                });
                TestResult::Battleship(game.play_game(agents, observer.clone()).await)
            }
        };

        if let Some(observer) = observer {
            let event = match result.error() {
                Some(message) => GameEvent::Error {
                    game_type: self.name().to_string(),
                    message: message.to_string(),
                },
                None => GameEvent::Complete {
                    game_type: self.name().to_string(),
                    result: serde_json::to_value(&result).unwrap_or_else(
                        |error| serde_json::json!({"serialization_error": error.to_string()}),
                    ),
                },
            };
            let _ = observer.send(event).await;
        }

        result
    }
}

impl TestResult {
    pub fn stats(&self) -> &GameStats {
        match self {
            TestResult::TicTacToe(result) => &result.stats,
            TestResult::RockPaperScissors(result) => &result.stats,
            TestResult::ConnectFour(result) => &result.stats,
            TestResult::Chess(result) => &result.stats,
            TestResult::Checkers(result) => &result.stats,
            TestResult::Battleship(result) => &result.stats,
        }
    }

    pub fn error(&self) -> Option<&str> {
        match self {
            TestResult::TicTacToe(result) => result.error.as_deref(),
            TestResult::RockPaperScissors(result) => result.error.as_deref(),
            TestResult::ConnectFour(result) => result.error.as_deref(),
            TestResult::Chess(result) => result.error.as_deref(),
            TestResult::Checkers(result) => result.error.as_deref(),
            TestResult::Battleship(result) => result.error.as_deref(),
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
        assert!(matches!(Game::from("Chess"), Game::Chess(_)));
        assert!(matches!(Game::from("Checkers"), Game::Checkers(_)));
        assert!(matches!(Game::from("Battleship"), Game::Battleship(_)));
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
        assert_eq!(Game::from("Chess").name(), "Chess");
        assert_eq!(Game::from("Checkers").name(), "Checkers");
        assert_eq!(Game::from("Battleship").name(), "Battleship");
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
    fn test_chess_config_default() {
        let config = ChessConfig::default();
        assert_eq!(config.start_fen, None);
    }

    #[test]
    fn test_new_game_config_defaults() {
        assert_eq!(CheckersConfig::default().max_turns, 200);
        assert_eq!(BattleshipConfig::default().max_turns, 160);
    }

    #[test]
    fn test_player_order_default() {
        let order = PlayerOrder::default();
        assert!(matches!(order, PlayerOrder::OrderInList));
    }
}
