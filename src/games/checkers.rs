use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::time::Instant;

use crate::agent::{AIAgent, MoveRequest};
use crate::games::events::GameEvent;
use crate::games::stats::{GameStats, TurnStats};

const BOARD_SIZE: usize = 8;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CheckersConfig {
    pub max_turns: u32,
}

impl Default for CheckersConfig {
    fn default() -> Self {
        Self { max_turns: 200 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CheckersPlayer {
    Black,
    Red,
}

impl CheckersPlayer {
    fn other(self) -> Self {
        match self {
            Self::Black => Self::Red,
            Self::Red => Self::Black,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Black => "Black",
            Self::Red => "Red",
        }
    }

    fn forward(self) -> i32 {
        match self {
            Self::Black => 1,
            Self::Red => -1,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Piece {
    pub player: CheckersPlayer,
    pub king: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Square {
    pub row: u32,
    pub col: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckersMove {
    pub from: Square,
    pub path: Vec<Square>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CheckersState {
    pub board: Vec<Vec<Option<Piece>>>,
    pub current_player: CheckersPlayer,
    pub turn_number: u32,
    pub game_over: bool,
    pub winner: Option<CheckersPlayer>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CheckersResult {
    pub winner: Option<String>,
    pub stats: GameStats,
    pub error: Option<String>,
}

pub struct Checkers {
    config: CheckersConfig,
    state: CheckersState,
    stats: GameStats,
    game_id: String,
}

impl Checkers {
    pub fn new(config: CheckersConfig) -> Self {
        let mut board = vec![vec![None; BOARD_SIZE]; BOARD_SIZE];
        for row in 0..3 {
            for col in 0..BOARD_SIZE {
                if (row + col) % 2 == 1 {
                    board[row][col] = Some(Piece {
                        player: CheckersPlayer::Black,
                        king: false,
                    });
                }
            }
        }
        for row in 5..BOARD_SIZE {
            for col in 0..BOARD_SIZE {
                if (row + col) % 2 == 1 {
                    board[row][col] = Some(Piece {
                        player: CheckersPlayer::Red,
                        king: false,
                    });
                }
            }
        }
        Self {
            config,
            state: CheckersState {
                board,
                current_player: CheckersPlayer::Black,
                turn_number: 0,
                game_over: false,
                winner: None,
            },
            stats: GameStats::new(),
            game_id: format!("checkers_{}", &uuid::Uuid::new_v4().to_string()[..8]),
        }
    }

    pub async fn play_game(
        mut self,
        agents: Vec<AIAgent>,
        observer: Option<tokio::sync::mpsc::Sender<GameEvent>>,
    ) -> CheckersResult {
        let started = Instant::now();
        if agents.len() != 2 {
            return CheckersResult {
                winner: None,
                stats: self.stats,
                error: Some(format!("Expected 2 agents, got {}", agents.len())),
            };
        }

        while !self.state.game_over && self.state.turn_number < self.config.max_turns {
            let player_index = match self.state.current_player {
                CheckersPlayer::Black => 0,
                CheckersPlayer::Red => 1,
            };
            let player = self.state.current_player;
            let agent = &agents[player_index];
            if self.execute_turn(agent, player).await.is_ok() {
                if self.player_has_won(player) {
                    self.state.game_over = true;
                    self.state.winner = Some(player);
                    self.stats.winner = Some(format!("{} ({})", agent.name(), player.as_str()));
                } else {
                    self.state.current_player = player.other();
                }
            }
            if let Some(observer) = &observer {
                let _ = observer
                    .send(GameEvent::turn(
                        "Checkers",
                        self.state.turn_number,
                        self.state_to_json(),
                    ))
                    .await;
            }
        }
        if !self.state.game_over {
            self.state.game_over = true;
            self.stats.draw = true;
        }
        self.stats.total_duration_ms = started.elapsed().as_millis() as u64;
        CheckersResult {
            winner: self.stats.winner.clone(),
            stats: self.stats,
            error: None,
        }
    }

    async fn execute_turn(
        &mut self,
        agent: &AIAgent,
        player: CheckersPlayer,
    ) -> Result<(), String> {
        let started = Instant::now();
        let state_before = self.state_to_json();
        let request = MoveRequest {
            turn_index: self.state.turn_number,
            game_id: self.game_id.clone(),
            state: state_before.clone(),
            expected_move_schema: json!({
                "type": "object",
                "properties": {
                    "from": {"type": "object", "properties": {"row": {"type": "integer", "minimum": 0, "maximum": 7}, "col": {"type": "integer", "minimum": 0, "maximum": 7}}, "required": ["row", "col"]},
                    "path": {"type": "array", "minItems": 1, "items": {"type": "object", "properties": {"row": {"type": "integer", "minimum": 0, "maximum": 7}, "col": {"type": "integer", "minimum": 0, "maximum": 7}}, "required": ["row", "col"]}}
                },
                "required": ["from", "path"]
            }),
            instructions: Some("Captures are mandatory. When a capture continues, include every landing square in path.".to_string()),
        };
        let response = agent
            .execute_turn(&request)
            .await
            .map_err(|error| format!("Agent error: {error}"))?;
        let move_value = response.chosen_move;
        let move_data: CheckersMove = serde_json::from_value(move_value.clone())
            .map_err(|error| format!("Invalid checkers move: {error}"))?;
        let legal_moves = self.legal_moves(player);
        let valid = legal_moves.contains(&move_data);
        let error = (!valid)
            .then(|| "Move is not legal; captures are mandatory when available".to_string());
        if valid {
            self.apply_move(&move_data)?;
        }
        let state_after = self.state_to_json();
        self.stats.add_turn(TurnStats {
            turn_number: self.state.turn_number,
            player: agent.name().to_string(),
            move_made: move_value,
            time_taken_ms: started.elapsed().as_millis() as u64,
            move_valid: valid,
            error_message: error.clone(),
            state_before,
            state_after,
            diagnostics: response.diagnostics,
        });
        self.state.turn_number += 1;
        error.map_or(Ok(()), Err)
    }

    fn legal_moves(&self, player: CheckersPlayer) -> Vec<CheckersMove> {
        let mut captures = Vec::new();
        let mut steps = Vec::new();
        for row in 0..BOARD_SIZE {
            for col in 0..BOARD_SIZE {
                let Some(piece) = self.state.board[row][col] else {
                    continue;
                };
                if piece.player != player {
                    continue;
                }
                let from = Square {
                    row: row as u32,
                    col: col as u32,
                };
                self.capture_paths(
                    &self.state.board,
                    from,
                    from,
                    piece,
                    Vec::new(),
                    &mut captures,
                );
                for (dr, dc) in Self::directions(piece) {
                    if let Some(to) = Self::offset(row, col, dr, dc)
                        && self.state.board[to.0][to.1].is_none()
                    {
                        steps.push(CheckersMove {
                            from,
                            path: vec![Square {
                                row: to.0 as u32,
                                col: to.1 as u32,
                            }],
                        });
                    }
                }
            }
        }
        if captures.is_empty() { steps } else { captures }
    }

    fn capture_paths(
        &self,
        board: &[Vec<Option<Piece>>],
        current: Square,
        origin: Square,
        piece: Piece,
        path: Vec<Square>,
        output: &mut Vec<CheckersMove>,
    ) {
        let mut extended = false;
        for (dr, dc) in Self::directions(piece) {
            let row = current.row as usize;
            let col = current.col as usize;
            let Some(mid) = Self::offset(row, col, dr, dc) else {
                continue;
            };
            let Some(landing) = Self::offset(row, col, dr * 2, dc * 2) else {
                continue;
            };
            if board[mid.0][mid.1].is_some_and(|other| other.player != piece.player)
                && board[landing.0][landing.1].is_none()
            {
                extended = true;
                let mut next = board.to_vec();
                next[row][col] = None;
                next[mid.0][mid.1] = None;
                next[landing.0][landing.1] = Some(piece);
                let mut next_path = path.clone();
                next_path.push(Square {
                    row: landing.0 as u32,
                    col: landing.1 as u32,
                });
                self.capture_paths(
                    &next,
                    Square {
                        row: landing.0 as u32,
                        col: landing.1 as u32,
                    },
                    origin,
                    piece,
                    next_path,
                    output,
                );
            }
        }
        if !path.is_empty() && !extended {
            output.push(CheckersMove { from: origin, path });
        }
    }

    fn apply_move(&mut self, move_data: &CheckersMove) -> Result<(), String> {
        let start = (move_data.from.row as usize, move_data.from.col as usize);
        let mut piece = self.state.board[start.0][start.1]
            .ok_or_else(|| "No piece at from square".to_string())?;
        self.state.board[start.0][start.1] = None;
        let mut current = start;
        for destination in &move_data.path {
            let next = (destination.row as usize, destination.col as usize);
            if current.0.abs_diff(next.0) == 2 {
                let middle = ((current.0 + next.0) / 2, (current.1 + next.1) / 2);
                self.state.board[middle.0][middle.1] = None;
            }
            current = next;
        }
        if (piece.player == CheckersPlayer::Black && current.0 == BOARD_SIZE - 1)
            || (piece.player == CheckersPlayer::Red && current.0 == 0)
        {
            piece.king = true;
        }
        self.state.board[current.0][current.1] = Some(piece);
        Ok(())
    }

    fn player_has_won(&self, player: CheckersPlayer) -> bool {
        let opponent = player.other();
        !self
            .state
            .board
            .iter()
            .flatten()
            .any(|piece| piece.is_some_and(|piece| piece.player == opponent))
            || self.legal_moves(opponent).is_empty()
    }

    fn directions(piece: Piece) -> Vec<(i32, i32)> {
        let vertical = if piece.king {
            vec![-1, 1]
        } else {
            vec![piece.player.forward()]
        };
        vertical
            .into_iter()
            .flat_map(|dr| [(dr, -1), (dr, 1)])
            .collect()
    }

    fn offset(row: usize, col: usize, dr: i32, dc: i32) -> Option<(usize, usize)> {
        let row = row as i32 + dr;
        let col = col as i32 + dc;
        ((0..BOARD_SIZE as i32).contains(&row) && (0..BOARD_SIZE as i32).contains(&col))
            .then_some((row as usize, col as usize))
    }

    fn state_to_json(&self) -> Value {
        let board = self
            .state
            .board
            .iter()
            .map(|row| {
                row.iter()
                    .map(|piece| {
                        piece.map(|piece| match (piece.player, piece.king) {
                            (CheckersPlayer::Black, false) => "Black",
                            (CheckersPlayer::Black, true) => "Black King",
                            (CheckersPlayer::Red, false) => "Red",
                            (CheckersPlayer::Red, true) => "Red King",
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        json!({
            "board": board,
            "current_player": self.state.current_player.as_str(),
            "turn_number": self.state.turn_number,
            "game_over": self.state.game_over,
            "winner": self.state.winner.map(CheckersPlayer::as_str),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_with_twelve_pieces_per_player() {
        let game = Checkers::new(CheckersConfig::default());
        let count = |player| {
            game.state
                .board
                .iter()
                .flatten()
                .filter(|piece| piece.is_some_and(|piece| piece.player == player))
                .count()
        };
        assert_eq!(count(CheckersPlayer::Black), 12);
        assert_eq!(count(CheckersPlayer::Red), 12);
    }

    #[test]
    fn capture_is_mandatory_and_can_chain() {
        let mut game = Checkers::new(CheckersConfig::default());
        game.state.board = vec![vec![None; BOARD_SIZE]; BOARD_SIZE];
        game.state.board[2][1] = Some(Piece {
            player: CheckersPlayer::Black,
            king: false,
        });
        game.state.board[3][2] = Some(Piece {
            player: CheckersPlayer::Red,
            king: false,
        });
        game.state.board[5][4] = Some(Piece {
            player: CheckersPlayer::Red,
            king: false,
        });
        let legal = game.legal_moves(CheckersPlayer::Black);
        assert_eq!(
            legal,
            vec![CheckersMove {
                from: Square { row: 2, col: 1 },
                path: vec![Square { row: 4, col: 3 }, Square { row: 6, col: 5 }]
            }]
        );
    }

    #[test]
    fn reaching_back_rank_promotes_a_king() {
        let mut game = Checkers::new(CheckersConfig::default());
        game.state.board = vec![vec![None; BOARD_SIZE]; BOARD_SIZE];
        game.state.board[6][1] = Some(Piece {
            player: CheckersPlayer::Black,
            king: false,
        });
        game.apply_move(&CheckersMove {
            from: Square { row: 6, col: 1 },
            path: vec![Square { row: 7, col: 0 }],
        })
        .unwrap();
        assert!(game.state.board[7][0].unwrap().king);
    }
}
