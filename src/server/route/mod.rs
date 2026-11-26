mod error;
mod games;
mod leaderboard;
mod matrix;
mod router;
mod runs;
mod secrets;
mod toolbench;
mod websocket;

pub(super) use router::build_router;

use crate::{games::game::Game, server::error::AppError};
use serde::Deserialize;

#[derive(Deserialize)]
struct GameQuery {
    #[serde(alias = "game_type")]
    game: Option<String>,
    page: Option<u32>,
}

impl GameQuery {
    fn selected_game(&self) -> Result<Game, AppError> {
        Game::new(self.game.as_deref().unwrap_or("TicTacToe"))
            .ok_or_else(|| AppError::BadRequest("Unknown game type".into()))
    }
}
