use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A typed, transport-neutral update emitted while a game is running.
///
/// The same payload is used by the CLI, the server's live observer, and the
/// WebSocket endpoint. Persisted runs retain the equivalent turn snapshots in
/// DuckDB so they can be replayed after the process exits.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum GameEvent {
    Turn {
        game_type: String,
        turn_number: u32,
        state: Value,
    },
    Complete {
        game_type: String,
        result: Value,
    },
    Error {
        game_type: String,
        message: String,
    },
}

impl GameEvent {
    pub fn turn(game_type: impl Into<String>, turn_number: u32, state: Value) -> Self {
        Self::Turn {
            game_type: game_type.into(),
            turn_number,
            state,
        }
    }

    pub fn state(&self) -> Option<&Value> {
        match self {
            Self::Turn { state, .. } => Some(state),
            Self::Complete { .. } | Self::Error { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn turn_event_serializes_with_a_discriminant_and_snapshot() {
        let event = GameEvent::turn("TicTacToe", 3, json!({"board": [["X"]]}));
        let value = serde_json::to_value(&event).unwrap();

        assert_eq!(value["type"], "turn");
        assert_eq!(value["game_type"], "TicTacToe");
        assert_eq!(value["turn_number"], 3);
        assert_eq!(event.state(), Some(&json!({"board": [["X"]]})));
    }
}
