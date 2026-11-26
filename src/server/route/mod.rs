mod error;
mod games;
mod leaderboard;
mod matrix;
mod router;
mod runs;
mod secrets;
mod toolbench;
mod turing;
mod websocket;

pub(super) use router::build_router;

use crate::{games::game::Game, server::error::AppError};
use axum::{
    http::{StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};

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

async fn private_response(request: axum::extract::Request, next: Next) -> Response {
    // Operator controls accept browser requests from this Arena.
    if let Some(origin) = request.headers().get(header::ORIGIN) {
        let authority = origin
            .to_str()
            .ok()
            .and_then(|s| s.parse::<axum::http::Uri>().ok());
        let host = request
            .headers()
            .get(header::HOST)
            .and_then(|v| v.to_str().ok())
            .or_else(|| request.uri().host());
        if !authority
            .as_ref()
            .and_then(|uri| uri.authority())
            .zip(host)
            .is_some_and(|(origin, host)| origin.as_str().eq_ignore_ascii_case(host))
        {
            return StatusCode::FORBIDDEN.into_response();
        }
    }
    let mut response = next.run(request).await;
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    response
}
