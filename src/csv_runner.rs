use csv::ReaderBuilder;
use std::fs::File;
use std::path::Path;

use crate::agent_config::{AIAgentConfig, AgentKind};
use crate::games::{Game, TestResult, print_game_stats};

#[derive(Debug, Clone)]
pub struct CsvTestCase {
    pub game_name: String,
    pub agent_one_kind: AgentKind,
    pub agent_one_model: String,
    pub agent_one_temp: f32,
    pub agent_one_seed: u64,
    pub agent_one_secret_profile: Option<String>,
    pub agent_two_kind: AgentKind,
    pub agent_two_model: String,
    pub agent_two_temp: f32,
    pub agent_two_seed: u64,
    pub agent_two_secret_profile: Option<String>,
    pub repetitions: u32,
    pub description: String,
}

impl CsvTestCase {
    fn from_record(record: csv::StringRecord, headers: &csv::StringRecord) -> Result<Self, String> {
        let get_field = |name: &str| -> Result<String, String> {
            let idx = headers
                .iter()
                .position(|h| h.eq_ignore_ascii_case(name))
                .ok_or_else(|| format!("Missing required field: {}", name))?;
            Ok(record.get(idx).unwrap_or("").to_string())
        };

        let parse_agent_kind = |name: &str| -> Result<AgentKind, String> {
            let value = get_field(name)?;
            match value.to_uppercase().as_str() {
                "OPENAI" => Ok(AgentKind::OpenAI),
                "ANTHROPIC" => Ok(AgentKind::Anthropic),
                "OLLAMA" => Ok(AgentKind::Ollama),
                _ => Err(format!("Invalid agent kind: {}. Must be OpenAI, Anthropic, or Ollama", value)),
            }
        };

        let parse_u32 = |name: &str| -> Result<u32, String> {
            get_field(name)?
                .parse()
                .map_err(|e| format!("Invalid {}: {}", name, e))
        };

        let parse_f32 = |name: &str| -> Result<f32, String> {
            get_field(name)?
                .parse()
                .map_err(|e| format!("Invalid {}: {}", name, e))
        };

        let parse_u64 = |name: &str| -> Result<u64, String> {
            get_field(name)?
                .parse()
                .map_err(|e| format!("Invalid {}: {}", name, e))
        };

        // Helper to get optional field
        let get_optional_field = |name: &str| -> Option<String> {
            headers
                .iter()
                .position(|h| h.eq_ignore_ascii_case(name))
                .and_then(|idx| {
                    record.get(idx)
                        .filter(|s| !s.is_empty())
                        .map(|s| s.to_string())
                })
        };

        Ok(CsvTestCase {
            game_name: get_field("game_name")?,
            agent_one_kind: parse_agent_kind("agent_one_kind")?,
            agent_one_model: get_field("agent_one_model")?,
            agent_one_temp: parse_f32("agent_one_temp").unwrap_or(0.7),
            agent_one_seed: parse_u64("agent_one_seed").unwrap_or(0),
            agent_one_secret_profile: get_optional_field("agent_one_secret_profile"),
            agent_two_kind: parse_agent_kind("agent_two_kind")?,
            agent_two_model: get_field("agent_two_model")?,
            agent_two_temp: parse_f32("agent_two_temp").unwrap_or(0.7),
            agent_two_seed: parse_u64("agent_two_seed").unwrap_or(0),
            agent_two_secret_profile: get_optional_field("agent_two_secret_profile"),
            repetitions: parse_u32("repetitions").unwrap_or(1),
            description: get_field("description").unwrap_or_else(|_| "".to_string()),
        })
    }

    pub fn to_agent_configs(&self) -> Vec<AIAgentConfig> {
        vec![
            AIAgentConfig {
                model: self.agent_one_model.clone(),
                temp: self.agent_one_temp,
                seed: Some(self.agent_one_seed),
                agent: self.agent_one_kind,
                secret_profile: self.agent_one_secret_profile.clone(),
            },
            AIAgentConfig {
                model: self.agent_two_model.clone(),
                temp: self.agent_two_temp,
                seed: Some(self.agent_two_seed),
                agent: self.agent_two_kind,
                secret_profile: self.agent_two_secret_profile.clone(),
            },
        ]
    }
}

pub fn read_csv_file<P: AsRef<Path>>(path: P) -> Result<Vec<CsvTestCase>, String> {
    let file = File::open(path).map_err(|e| format!("Failed to open CSV file: {}", e))?;
    let mut reader = ReaderBuilder::new()
        .has_headers(true)
        .flexible(true)
        .from_reader(file);

    let headers = reader
        .headers()
        .map_err(|e| format!("Failed to read CSV headers: {}", e))?
        .clone();

    let mut test_cases = Vec::new();
    for (row_num, result) in reader.records().enumerate() {
        let record = result.map_err(|e| format!("Failed to read CSV row {}: {}", row_num + 2, e))?;
        match CsvTestCase::from_record(record, &headers) {
            Ok(test_case) => test_cases.push(test_case),
            Err(e) => return Err(format!("Error parsing row {}: {}", row_num + 2, e)),
        }
    }

    Ok(test_cases)
}

pub async fn run_csv_batch(csv_path: &str, verbose: bool) -> Result<(), String> {
    let test_cases = read_csv_file(csv_path)?;
    
    println!("\n{}", "=".repeat(80));
    println!("CSV BATCH RUN");
    println!("Found {} test case(s) in CSV file", test_cases.len());
    println!("{}", "=".repeat(80));

    let mut total_games = 0;
    let mut completed_games = 0;

    for (idx, test_case) in test_cases.iter().enumerate() {
        println!("\n[Test Case {} of {}]", idx + 1, test_cases.len());
        if !test_case.description.is_empty() {
            println!("Description: {}", test_case.description);
        }
        println!("Game: {}", test_case.game_name);
        println!("Repetitions: {}", test_case.repetitions);
        println!("Agents: {} ({}) vs {} ({})", 
            test_case.agent_one_model, 
            format!("{:?}", test_case.agent_one_kind),
            test_case.agent_two_model,
            format!("{:?}", test_case.agent_two_kind));

        let game = Game::from(test_case.game_name.as_str());
        let agents = test_case.to_agent_configs();

        for rep in 0..test_case.repetitions {
            total_games += 1;
            
            if test_case.repetitions > 1 {
                println!("\n--- Repetition {} of {} ---", rep + 1, test_case.repetitions);
            }

            match game.play_game(agents.clone()).await {
                result => {
                    completed_games += 1;
                    if verbose || test_case.repetitions == 1 {
                        print_game_stats(game.name(), &result);
                    } else {
                        // Brief summary for multiple repetitions
                        let winner = match &result {
                            TestResult::TicTacToe(r) => r.winner.as_ref(),
                            TestResult::RockPaperScissors(r) => r.winner.as_ref(),
                            TestResult::ConnectFour(r) => r.winner.as_ref(),
                        };
                        println!("  Result: {}", 
                            winner.map(|w| format!("Winner: {}", w))
                                .unwrap_or_else(|| "Draw".to_string()));
                    }
                }
            }
        }
    }

    println!("\n{}", "=".repeat(80));
    println!("BATCH RUN COMPLETE");
    println!("Total games: {}", total_games);
    println!("Completed: {}", completed_games);
    println!("{}", "=".repeat(80));

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_headers() -> csv::StringRecord {
        csv::StringRecord::from(vec![
            "game_name",
            "agent_one_kind",
            "agent_one_model",
            "agent_one_temp",
            "agent_one_seed",
            "agent_one_secret_profile",
            "agent_two_kind",
            "agent_two_model",
            "agent_two_temp",
            "agent_two_seed",
            "agent_two_secret_profile",
            "repetitions",
            "description",
        ])
    }

    #[test]
    fn test_csv_test_case_from_record_minimal() {
        let headers = create_test_headers();
        let record = csv::StringRecord::from(vec![
            "TicTacToe",
            "OpenAI",
            "gpt-4o-mini",
            "", // temp defaults to 0.7
            "", // seed defaults to 0
            "",
            "Ollama",
            "llama3",
            "", // temp defaults to 0.7
            "", // seed defaults to 0
            "",
            "", // repetitions defaults to 1
            "Test game",
        ]);

        let result = CsvTestCase::from_record(record, &headers);
        assert!(result.is_ok());
        let test_case = result.unwrap();
        
        assert_eq!(test_case.game_name, "TicTacToe");
        assert_eq!(test_case.agent_one_kind, AgentKind::OpenAI);
        assert_eq!(test_case.agent_one_model, "gpt-4o-mini");
        assert_eq!(test_case.agent_one_temp, 0.7);
        assert_eq!(test_case.agent_one_seed, 0);
        assert_eq!(test_case.agent_two_kind, AgentKind::Ollama);
        assert_eq!(test_case.agent_two_model, "llama3");
        assert_eq!(test_case.agent_two_temp, 0.7);
        assert_eq!(test_case.agent_two_seed, 0);
        assert_eq!(test_case.repetitions, 1);
        assert_eq!(test_case.description, "Test game");
    }

    #[test]
    fn test_csv_test_case_from_record_full() {
        let headers = create_test_headers();
        let record = csv::StringRecord::from(vec![
            "ConnectFour",
            "Anthropic",
            "claude-3-7-sonnet",
            "0.5",
            "42",
            "profile1",
            "OpenAI",
            "gpt-4o-mini",
            "0.9",
            "43",
            "profile2",
            "3",
            "Full test",
        ]);

        let result = CsvTestCase::from_record(record, &headers);
        assert!(result.is_ok());
        let test_case = result.unwrap();
        
        assert_eq!(test_case.game_name, "ConnectFour");
        assert_eq!(test_case.agent_one_kind, AgentKind::Anthropic);
        assert_eq!(test_case.agent_one_model, "claude-3-7-sonnet");
        assert_eq!(test_case.agent_one_temp, 0.5);
        assert_eq!(test_case.agent_one_seed, 42);
        assert_eq!(test_case.agent_one_secret_profile, Some("profile1".to_string()));
        assert_eq!(test_case.agent_two_kind, AgentKind::OpenAI);
        assert_eq!(test_case.agent_two_model, "gpt-4o-mini");
        assert_eq!(test_case.agent_two_temp, 0.9);
        assert_eq!(test_case.agent_two_seed, 43);
        assert_eq!(test_case.agent_two_secret_profile, Some("profile2".to_string()));
        assert_eq!(test_case.repetitions, 3);
        assert_eq!(test_case.description, "Full test");
    }

    #[test]
    fn test_csv_test_case_from_record_case_insensitive_agent_kind() {
        let headers = create_test_headers();
        let record = csv::StringRecord::from(vec![
            "TicTacToe",
            "openai", // lowercase
            "gpt-4o-mini",
            "",
            "",
            "",
            "OLLAMA", // uppercase
            "llama3",
            "",
            "",
            "",
            "",
            "",
        ]);

        let result = CsvTestCase::from_record(record, &headers);
        assert!(result.is_ok());
        let test_case = result.unwrap();
        assert_eq!(test_case.agent_one_kind, AgentKind::OpenAI);
        assert_eq!(test_case.agent_two_kind, AgentKind::Ollama);
    }

    #[test]
    fn test_csv_test_case_from_record_invalid_agent_kind() {
        let headers = create_test_headers();
        let record = csv::StringRecord::from(vec![
            "TicTacToe",
            "InvalidAgent",
            "model",
            "",
            "",
            "",
            "OpenAI",
            "model",
            "",
            "",
            "",
            "",
            "",
        ]);

        let result = CsvTestCase::from_record(record, &headers);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid agent kind"));
    }

    #[test]
    fn test_csv_test_case_from_record_missing_required_field() {
        let headers = csv::StringRecord::from(vec!["game_name", "agent_one_kind"]);
        let record = csv::StringRecord::from(vec!["TicTacToe", "OpenAI"]);

        let result = CsvTestCase::from_record(record, &headers);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing required field"));
    }

    #[test]
    fn test_csv_test_case_to_agent_configs() {
        let test_case = CsvTestCase {
            game_name: "TicTacToe".to_string(),
            agent_one_kind: AgentKind::OpenAI,
            agent_one_model: "gpt-4o-mini".to_string(),
            agent_one_temp: 0.7,
            agent_one_seed: 42,
            agent_one_secret_profile: Some("profile1".to_string()),
            agent_two_kind: AgentKind::Ollama,
            agent_two_model: "llama3".to_string(),
            agent_two_temp: 0.8,
            agent_two_seed: 43,
            agent_two_secret_profile: None,
            repetitions: 1,
            description: "Test".to_string(),
        };

        let configs = test_case.to_agent_configs();
        assert_eq!(configs.len(), 2);
        
        assert_eq!(configs[0].model, "gpt-4o-mini");
        assert_eq!(configs[0].temp, 0.7);
        assert_eq!(configs[0].seed, Some(42));
        assert_eq!(configs[0].agent, AgentKind::OpenAI);
        assert_eq!(configs[0].secret_profile, Some("profile1".to_string()));
        
        assert_eq!(configs[1].model, "llama3");
        assert_eq!(configs[1].temp, 0.8);
        assert_eq!(configs[1].seed, Some(43));
        assert_eq!(configs[1].agent, AgentKind::Ollama);
        assert_eq!(configs[1].secret_profile, None);
    }
}

