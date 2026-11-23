use serde_json::json;

use crate::agent::MoveRequest;

pub fn system_prompt(request: &MoveRequest, custom_prompt: Option<&str>) -> String {
    let mut system = "You are a game-playing AI. Respond with only JSON matching the expected schema. Use the gamestate to make only valid moves. Do not include any text outside JSON.".to_string();

    if let Some(instructions) = &request.instructions {
        system.push_str(" ");
        system.push_str(instructions);
    }

    if let Some(custom_prompt) = custom_prompt.filter(|prompt| !prompt.trim().is_empty()) {
        system.push_str(" Additional operator guidance: ");
        system.push_str(custom_prompt);
    }
    system
}

pub fn user_prompt(request: &MoveRequest) -> String {
    json!({
        "turn_index": request.turn_index,
        "game_id": request.game_id,
        "game_state": request.state,
        "expected_move_schema": request.expected_move_schema,
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn custom_prompt_is_appended_to_system_guidance() {
        let request = MoveRequest {
            turn_index: 0,
            game_id: "test".to_string(),
            state: json!({}),
            expected_move_schema: json!({}),
            instructions: None,
        };
        assert!(
            system_prompt(&request, Some("Favor center control."))
                .contains("Favor center control.")
        );
    }
}
