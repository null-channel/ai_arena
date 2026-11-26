use crate::games::events::GameEvent;
use crate::server::{sessions::GameStatus, state::AppState};
use axum::{
    Router,
    extract::{
        Path, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    response::IntoResponse,
    routing::get,
};
use futures_util::{FutureExt, SinkExt, StreamExt};
use tokio::sync::mpsc;

pub(super) fn routes() -> Router<AppState> {
    Router::new().route("/ws/games/{id}", get(game_websocket))
}

async fn game_websocket(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Path(game_id): Path<String>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_game_socket(socket, state, game_id))
}

async fn handle_game_socket(socket: WebSocket, state: AppState, game_id: String) {
    let (mut sender, _receiver) = socket.split();

    // Create a channel for game state updates
    let (tx, mut rx) = mpsc::channel::<GameEvent>(100);

    // Get the game session
    let (game, agents, mode) = {
        let mut games = state.games.lock().await;
        let session = match games.get_mut(&game_id) {
            Some(s) => s,
            None => {
                let _ = sender
                    .send(Message::Text(
                        serde_json::json!({"error": "Game not found"})
                            .to_string()
                            .into(),
                    ))
                    .await;
                return;
            }
        };

        if !matches!(session.status, GameStatus::Pending) {
            let _ = sender
                .send(Message::Text(
                    serde_json::json!({"error": "Game already started or completed"})
                        .to_string()
                        .into(),
                ))
                .await;
            return;
        }

        session.status = GameStatus::Running;
        (session.game.clone(), session.agents.clone(), session.mode)
    };

    if let Err(error) = state.result_store(mode).mark_running(&game_id).await {
        eprintln!(
            "Failed to mark DuckDB run {} as running: {}",
            game_id, error
        );
    }

    // Spawn a task to run the game
    let state_clone = state.clone();
    let game_id_clone = game_id.clone();
    tokio::spawn(async move {
        let secrets = {
            let secrets = state_clone.secrets.lock().await;
            secrets.clone()
        };
        let result = std::panic::AssertUnwindSafe(game.play_game(agents, Some(tx), Some(&secrets)))
            .catch_unwind()
            .await;

        match &result {
            Ok(result) => {
                if let Err(error) = state_clone
                    .result_store(mode)
                    .complete_run(&game_id_clone, result)
                    .await
                {
                    eprintln!("Failed to store DuckDB run {}: {}", game_id_clone, error);
                }
            }
            Err(_) => {
                if let Err(error) = state_clone
                    .result_store(mode)
                    .mark_error(&game_id_clone, "game execution panicked")
                    .await
                {
                    eprintln!(
                        "Failed to mark DuckDB run {} as error: {}",
                        game_id_clone, error
                    );
                }
            }
        }

        // Update the game session with the result
        let mut games = state_clone.games.lock().await;
        if let Some(session) = games.get_mut(&game_id_clone) {
            match result {
                Ok(result) => session.finish(result),
                Err(_) => {
                    session.status = GameStatus::Error;
                    session.result = None;
                }
            }
        }
    });

    // Forward game state updates to websocket
    while let Some(event) = rx.recv().await {
        {
            let mut games = state.games.lock().await;
            if let Some(session) = games.get_mut(&game_id) {
                session.record_event(event.clone());
            }
        }
        let message_text = serde_json::to_string(&event).unwrap_or_else(|error| {
            serde_json::json!({"type": "error", "message": error.to_string()}).to_string()
        });
        if sender
            .send(Message::Text(message_text.into()))
            .await
            .is_err()
        {
            break;
        }
    }

    // Send final completion message
    let _ = sender
        .send(Message::Text(
            serde_json::json!({"type": "complete"}).to_string().into(),
        ))
        .await;
}
