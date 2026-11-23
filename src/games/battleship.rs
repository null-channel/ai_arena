use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::time::Instant;

use crate::agent::{AIAgent, MoveRequest};
use crate::games::events::GameEvent;
use crate::games::stats::{GameStats, TurnStats};

const BOARD_SIZE: usize = 8;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BattleshipConfig {
    pub max_turns: u32,
}

impl Default for BattleshipConfig {
    fn default() -> Self {
        Self { max_turns: 160 }
    }
}

#[derive(Clone, Debug)]
struct FleetBoard {
    ships: Vec<Vec<bool>>,
    shots_received: Vec<Vec<Option<bool>>>,
}

impl FleetBoard {
    fn new(ships: &[(usize, usize)]) -> Self {
        let mut grid = vec![vec![false; BOARD_SIZE]; BOARD_SIZE];
        for &(row, col) in ships {
            grid[row][col] = true;
        }
        Self {
            ships: grid,
            shots_received: vec![vec![None; BOARD_SIZE]; BOARD_SIZE],
        }
    }

    fn take_shot(&mut self, row: usize, col: usize) -> Result<bool, String> {
        if row >= BOARD_SIZE || col >= BOARD_SIZE {
            return Err("Shot is outside the board".to_string());
        }
        if self.shots_received[row][col].is_some() {
            return Err("That coordinate was already targeted".to_string());
        }
        let hit = self.ships[row][col];
        self.shots_received[row][col] = Some(hit);
        Ok(hit)
    }

    fn all_sunk(&self) -> bool {
        self.ships.iter().enumerate().all(|(row, cells)| {
            cells
                .iter()
                .enumerate()
                .all(|(col, ship)| !ship || self.shots_received[row][col] == Some(true))
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BattleshipResult {
    pub winner: Option<String>,
    pub stats: GameStats,
    pub error: Option<String>,
}

pub struct Battleship {
    config: BattleshipConfig,
    boards: [FleetBoard; 2],
    current_player: usize,
    turn_number: u32,
    game_over: bool,
    stats: GameStats,
    game_id: String,
}

impl Battleship {
    pub fn new(config: BattleshipConfig) -> Self {
        // Deterministic, mirrored fleets make a run reproducible while keeping the
        // opponent fleet hidden from the active agent's state.
        let player_one = FleetBoard::new(&[(0, 0), (0, 1), (0, 2), (2, 1), (2, 2), (4, 4), (4, 5)]);
        let player_two = FleetBoard::new(&[(7, 5), (7, 6), (7, 7), (5, 5), (5, 6), (3, 2), (3, 3)]);
        Self {
            config,
            boards: [player_one, player_two],
            current_player: 0,
            turn_number: 0,
            game_over: false,
            stats: GameStats::new(),
            game_id: format!("battleship_{}", &uuid::Uuid::new_v4().to_string()[..8]),
        }
    }

    pub async fn play_game(
        mut self,
        agents: Vec<AIAgent>,
        observer: Option<tokio::sync::mpsc::Sender<GameEvent>>,
    ) -> BattleshipResult {
        let started = Instant::now();
        if agents.len() != 2 {
            return BattleshipResult {
                winner: None,
                stats: self.stats,
                error: Some(format!("Expected 2 agents, got {}", agents.len())),
            };
        }
        while !self.game_over && self.turn_number < self.config.max_turns {
            let player = self.current_player;
            let agent = &agents[player];
            if self.execute_turn(agent).await.is_ok() {
                if self.boards[1 - player].all_sunk() {
                    self.game_over = true;
                    self.stats.winner = Some(format!("{} (Player {})", agent.name(), player + 1));
                } else {
                    self.current_player = 1 - player;
                }
            }
            if let Some(observer) = &observer {
                let _ = observer
                    .send(GameEvent::turn(
                        "Battleship",
                        self.turn_number,
                        self.state_to_json(player),
                    ))
                    .await;
            }
        }
        if !self.game_over {
            self.stats.draw = true;
        }
        self.stats.total_duration_ms = started.elapsed().as_millis() as u64;
        BattleshipResult {
            winner: self.stats.winner.clone(),
            stats: self.stats,
            error: None,
        }
    }

    async fn execute_turn(&mut self, agent: &AIAgent) -> Result<(), String> {
        let started = Instant::now();
        let player = self.current_player;
        let state_before = self.state_to_json(player);
        let request = MoveRequest {
            turn_index: self.turn_number,
            game_id: self.game_id.clone(),
            state: state_before.clone(),
            expected_move_schema: json!({
                "type": "object",
                "properties": {
                    "row": {"type": "integer", "minimum": 0, "maximum": 7},
                    "col": {"type": "integer", "minimum": 0, "maximum": 7}
                },
                "required": ["row", "col"]
            }),
            instructions: Some(
                "Choose one untargeted coordinate. Your opponent's intact ships are hidden."
                    .to_string(),
            ),
        };
        let response = agent
            .execute_turn(&request)
            .await
            .map_err(|error| format!("Agent error: {error}"))?;
        let move_value = response.chosen_move;
        let row = move_value
            .get("row")
            .and_then(Value::as_u64)
            .map(|value| value as usize);
        let col = move_value
            .get("col")
            .and_then(Value::as_u64)
            .map(|value| value as usize);
        let shot = row
            .zip(col)
            .ok_or_else(|| "Move requires integer row and col".to_string());
        let outcome = shot.and_then(|(row, col)| self.boards[1 - player].take_shot(row, col));
        let valid = outcome.is_ok();
        let error = outcome.err();
        let state_after = self.state_to_json(player);
        self.stats.add_turn(TurnStats {
            turn_number: self.turn_number,
            player: agent.name().to_string(),
            move_made: move_value,
            time_taken_ms: started.elapsed().as_millis() as u64,
            move_valid: valid,
            error_message: error.clone(),
            state_before,
            state_after,
            diagnostics: response.diagnostics,
        });
        self.turn_number += 1;
        error.map_or(Ok(()), Err)
    }

    fn state_to_json(&self, viewer: usize) -> Value {
        let own = &self.boards[viewer];
        let opponent = &self.boards[1 - viewer];
        let own_board = (0..BOARD_SIZE)
            .map(|row| {
                (0..BOARD_SIZE)
                    .map(
                        |col| match (own.ships[row][col], own.shots_received[row][col]) {
                            (true, Some(true)) => "Hit ship",
                            (true, _) => "Ship",
                            (false, Some(false)) => "Miss",
                            (false, Some(true)) => "Hit",
                            (false, None) => "Water",
                        },
                    )
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let opponent_board = (0..BOARD_SIZE)
            .map(|row| {
                (0..BOARD_SIZE)
                    .map(|col| match opponent.shots_received[row][col] {
                        Some(true) => "Hit",
                        Some(false) => "Miss",
                        None => "Unknown",
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        json!({
            "own_board": own_board,
            "opponent_board": opponent_board,
            "current_player": format!("Player {}", self.current_player + 1),
            "turn_number": self.turn_number,
            "board_size": BOARD_SIZE,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_fleets_have_seven_ship_cells() {
        let game = Battleship::new(BattleshipConfig::default());
        for board in &game.boards {
            assert_eq!(
                board.ships.iter().flatten().filter(|ship| **ship).count(),
                7
            );
        }
    }

    #[test]
    fn public_opponent_view_hides_intact_ships() {
        let game = Battleship::new(BattleshipConfig::default());
        let state = game.state_to_json(0);
        assert_eq!(state["opponent_board"][7][5], "Unknown");
        assert_eq!(state["own_board"][0][0], "Ship");
    }

    #[test]
    fn repeated_shots_are_rejected_and_sunk_fleet_is_detected() {
        let mut board = FleetBoard::new(&[(0, 0)]);
        assert!(board.take_shot(0, 0).unwrap());
        assert!(board.all_sunk());
        assert!(board.take_shot(0, 0).is_err());
    }
}
