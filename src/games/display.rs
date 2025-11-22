use tabled::{Table, Tabled, settings::{Style, Alignment, Modify, object::Rows}};
use serde_json::Value;

use super::stats::GameStats;
use super::game::TestResult;

/// Display game statistics in a formatted table
pub fn print_game_stats(game_name: &str, result: &TestResult) {
    println!("\n{}", "=".repeat(80));
    println!("GAME RESULTS: {}", game_name);
    println!("{}", "=".repeat(80));

    match result {
        TestResult::TicTacToe(result) => {
            print_game_summary(&result.stats, result.error.as_deref());
            print_turn_table(&result.stats);
            print_player_summary(&result.stats);
        }
        TestResult::RockPaperScissors(result) => {
            print_game_summary(&result.stats, result.error.as_deref());
            print_turn_table(&result.stats);
            print_player_summary(&result.stats);
        }
        TestResult::ConnectFour(result) => {
            print_game_summary(&result.stats, result.error.as_deref());
            print_turn_table(&result.stats);
            print_player_summary(&result.stats);
        }
    }

    println!("\n{}", "=".repeat(80));
}

fn print_game_summary(stats: &GameStats, error: Option<&str>) {
    println!("\n📊 GAME SUMMARY");
    println!("{}", "-".repeat(80));
    
    if let Some(err) = error {
        println!("❌ Error: {}", err);
        return;
    }

    match &stats.winner {
        Some(winner) => println!("🏆 Winner: {}", winner),
        None if stats.draw => println!("🤝 Result: Draw"),
        None => println!("⚠️  Result: Incomplete"),
    }

    println!("⏱️  Total Duration: {:.2}s", stats.total_duration_ms as f64 / 1000.0);
    println!("🔄 Total Turns: {}", stats.total_turns());
    println!("⚡ Average Turn Time: {:.2}ms", stats.average_turn_time_ms());
    println!("❌ Invalid Moves: {}", stats.invalid_moves);
}

#[derive(Tabled)]
struct TurnRow {
    #[tabled(rename = "Turn")]
    turn: String,
    #[tabled(rename = "Player")]
    player: String,
    #[tabled(rename = "Move")]
    move_str: String,
    #[tabled(rename = "Time (ms)")]
    time: String,
    #[tabled(rename = "Valid")]
    valid: String,
    #[tabled(rename = "Error")]
    error: String,
}

fn print_turn_table(stats: &GameStats) {
    if stats.turns.is_empty() {
        return;
    }

    println!("\n📋 TURN-BY-TURN STATISTICS");
    println!("{}", "-".repeat(80));

    let rows: Vec<TurnRow> = stats.turns.iter().map(|turn| {
        TurnRow {
            turn: turn.turn_number.to_string(),
            player: turn.player.clone(),
            move_str: format_move(&turn.move_made),
            time: turn.time_taken_ms.to_string(),
            valid: if turn.move_valid { "✓".to_string() } else { "✗".to_string() },
            error: turn.error_message.as_ref()
                .map(|e| e.chars().take(30).collect::<String>())
                .unwrap_or_else(|| "-".to_string()),
        }
    }).collect();

    let mut table = Table::new(rows);
    table
        .with(Style::rounded())
        .with(Modify::new(Rows::new(1..)).with(Alignment::left()));

    println!("{}", table);
}

fn print_player_summary(stats: &GameStats) {
    if stats.turns.is_empty() {
        return;
    }

    println!("\n👥 PLAYER STATISTICS");
    println!("{}", "-".repeat(80));

    // Group turns by player
    use std::collections::HashMap;
    let mut player_stats: HashMap<String, PlayerStats> = HashMap::new();

    for turn in &stats.turns {
        let player_stat = player_stats.entry(turn.player.clone()).or_insert_with(|| PlayerStats {
            name: turn.player.clone(),
            total_turns: 0,
            valid_moves: 0,
            invalid_moves: 0,
            total_time_ms: 0,
            avg_time_ms: 0.0,
        });

        player_stat.total_turns += 1;
        if turn.move_valid {
            player_stat.valid_moves += 1;
        } else {
            player_stat.invalid_moves += 1;
        }
        player_stat.total_time_ms += turn.time_taken_ms;
    }

    // Calculate averages
    for stat in player_stats.values_mut() {
        if stat.total_turns > 0 {
            stat.avg_time_ms = stat.total_time_ms as f64 / stat.total_turns as f64;
        }
    }

    // Create table
    #[derive(Tabled)]
    struct PlayerRow {
        #[tabled(rename = "Player")]
        name: String,
        #[tabled(rename = "Total Turns")]
        total_turns: String,
        #[tabled(rename = "Valid Moves")]
        valid_moves: String,
        #[tabled(rename = "Invalid Moves")]
        invalid_moves: String,
        #[tabled(rename = "Total Time (ms)")]
        total_time: String,
        #[tabled(rename = "Avg Time (ms)")]
        avg_time: String,
    }

    let mut player_rows: Vec<PlayerRow> = player_stats.values().map(|stat| {
        PlayerRow {
            name: stat.name.clone(),
            total_turns: stat.total_turns.to_string(),
            valid_moves: stat.valid_moves.to_string(),
            invalid_moves: stat.invalid_moves.to_string(),
            total_time: stat.total_time_ms.to_string(),
            avg_time: format!("{:.2}", stat.avg_time_ms),
        }
    }).collect();

    // Sort by player name for consistency
    player_rows.sort_by_key(|r| r.name.clone());

    let mut table = Table::new(player_rows);
    table
        .with(Style::rounded())
        .with(Modify::new(Rows::new(1..)).with(Alignment::left()));

    println!("{}", table);
}

struct PlayerStats {
    name: String,
    total_turns: u32,
    valid_moves: u32,
    invalid_moves: u32,
    total_time_ms: u64,
    avg_time_ms: f64,
}

fn format_move(move_value: &Value) -> String {
    // Try to format the move nicely
    if let Some(obj) = move_value.as_object() {
        let parts: Vec<String> = obj.iter()
            .map(|(k, v)| {
                let val_str = match v {
                    Value::String(s) => s.clone(),
                    Value::Number(n) => n.to_string(),
                    _ => format!("{}", v),
                };
                format!("{}: {}", k, val_str)
            })
            .collect();
        parts.join(", ")
    } else {
        format!("{}", move_value)
    }
}

