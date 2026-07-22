use csv::ReaderBuilder;
use std::fs::File;
use std::path::Path;
use std::str::FromStr;

use crate::agent_config::{AIAgentConfig, AgentKind};
use crate::games::{Game, print_game_stats};
use crate::report::{JsonlWriter, MatchRecord, OutcomeSummary, agents_for_repetition};

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
        let parse_agent_kind = |name: &str| -> Result<AgentKind, String> {
            let value = required_field(&record, headers, name)?;
            match value.to_uppercase().as_str() {
                "OPENAI" => Ok(AgentKind::OpenAI),
                "ANTHROPIC" => Ok(AgentKind::Anthropic),
                "OLLAMA" => Ok(AgentKind::Ollama),
                _ => Err(format!(
                    "Invalid agent kind: {}. Must be OpenAI, Anthropic, or Ollama",
                    value
                )),
            }
        };

        let game_name = required_field(&record, headers, "game_name")?;
        Game::try_from(game_name.as_str())?;
        let agent_one_temp: f32 = parse_optional(&record, headers, "agent_one_temp", 0.7)?;
        let agent_two_temp: f32 = parse_optional(&record, headers, "agent_two_temp", 0.7)?;
        for (field, temperature) in [
            ("agent_one_temp", agent_one_temp),
            ("agent_two_temp", agent_two_temp),
        ] {
            if !temperature.is_finite() || !(0.0..=2.0).contains(&temperature) {
                return Err(format!("Invalid {field}: must be between 0.0 and 2.0"));
            }
        }
        let repetitions = parse_optional(&record, headers, "repetitions", 1)?;
        if repetitions == 0 {
            return Err("Invalid repetitions: must be greater than zero".into());
        }

        Ok(CsvTestCase {
            game_name,
            agent_one_kind: parse_agent_kind("agent_one_kind")?,
            agent_one_model: required_field(&record, headers, "agent_one_model")?,
            agent_one_temp,
            agent_one_seed: parse_optional(&record, headers, "agent_one_seed", 0)?,
            agent_one_secret_profile: optional_field(&record, headers, "agent_one_secret_profile"),
            agent_two_kind: parse_agent_kind("agent_two_kind")?,
            agent_two_model: required_field(&record, headers, "agent_two_model")?,
            agent_two_temp,
            agent_two_seed: parse_optional(&record, headers, "agent_two_seed", 0)?,
            agent_two_secret_profile: optional_field(&record, headers, "agent_two_secret_profile"),
            repetitions,
            description: optional_field(&record, headers, "description").unwrap_or_default(),
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

fn optional_field(
    record: &csv::StringRecord,
    headers: &csv::StringRecord,
    name: &str,
) -> Option<String> {
    headers
        .iter()
        .position(|header| header.eq_ignore_ascii_case(name))
        .and_then(|index| record.get(index))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn required_field(
    record: &csv::StringRecord,
    headers: &csv::StringRecord,
    name: &str,
) -> Result<String, String> {
    optional_field(record, headers, name)
        .ok_or_else(|| format!("Missing or empty required field: {name}"))
}

fn parse_optional<T>(
    record: &csv::StringRecord,
    headers: &csv::StringRecord,
    name: &str,
    default: T,
) -> Result<T, String>
where
    T: FromStr,
    T::Err: std::fmt::Display,
{
    optional_field(record, headers, name).map_or(Ok(default), |value| {
        value
            .parse()
            .map_err(|error| format!("Invalid {name}: {error}"))
    })
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
        let record =
            result.map_err(|e| format!("Failed to read CSV row {}: {}", row_num + 2, e))?;
        match CsvTestCase::from_record(record, &headers) {
            Ok(test_case) => test_cases.push(test_case),
            Err(e) => return Err(format!("Error parsing row {}: {}", row_num + 2, e)),
        }
    }

    Ok(test_cases)
}

pub async fn run_csv_batch(
    csv_path: &str,
    verbose: bool,
    mut output: Option<&mut JsonlWriter>,
) -> Result<(), String> {
    let test_cases = read_csv_file(csv_path)?;

    println!("\n{}", "=".repeat(80));
    println!("CSV BATCH RUN");
    println!("Found {} test case(s) in CSV file", test_cases.len());
    println!("{}", "=".repeat(80));

    let mut total_games = 0;
    let mut completed_games = 0;
    let mut summary = OutcomeSummary::default();

    for (idx, test_case) in test_cases.iter().enumerate() {
        println!("\n[Test Case {} of {}]", idx + 1, test_cases.len());
        if !test_case.description.is_empty() {
            println!("Description: {}", test_case.description);
        }
        println!("Game: {}", test_case.game_name);
        println!("Repetitions: {}", test_case.repetitions);
        println!(
            "Agents: {} ({:?}) vs {} ({:?})",
            test_case.agent_one_model,
            test_case.agent_one_kind,
            test_case.agent_two_model,
            test_case.agent_two_kind
        );

        let game = Game::try_from(test_case.game_name.as_str())?;
        let agents = test_case.to_agent_configs();

        for rep in 0..test_case.repetitions {
            total_games += 1;

            if test_case.repetitions > 1 {
                println!(
                    "\n--- Repetition {} of {} ---",
                    rep + 1,
                    test_case.repetitions
                );
            }

            let match_agents = agents_for_repetition(&agents, rep + 1);
            let result = game.play_game(match_agents.clone()).await;
            completed_games += 1;
            summary.record(&result.stats().outcome);
            if verbose || test_case.repetitions == 1 {
                print_game_stats(game.name(), &result);
            } else {
                // Brief summary for multiple repetitions
                let outcome = &result.stats().outcome;
                if let Some(winner) = outcome.winner() {
                    println!("  Result: Winner: {winner}");
                } else {
                    println!("  Result: {outcome:?}");
                }
            }
            if let Some(writer) = output.as_deref_mut() {
                let record = MatchRecord::new(
                    game.name(),
                    rep + 1,
                    test_case.repetitions,
                    &match_agents,
                    &result,
                );
                writer.write(&record)?;
            }
        }
    }

    println!("\n{}", "=".repeat(80));
    println!("BATCH RUN COMPLETE");
    println!("Total games: {}", total_games);
    println!("Completed: {}", completed_games);
    summary.print();
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
        assert_eq!(
            test_case.agent_one_secret_profile,
            Some("profile1".to_string())
        );
        assert_eq!(test_case.agent_two_kind, AgentKind::OpenAI);
        assert_eq!(test_case.agent_two_model, "gpt-4o-mini");
        assert_eq!(test_case.agent_two_temp, 0.9);
        assert_eq!(test_case.agent_two_seed, 43);
        assert_eq!(
            test_case.agent_two_secret_profile,
            Some("profile2".to_string())
        );
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
        assert!(
            result
                .unwrap_err()
                .contains("Missing or empty required field")
        );
    }

    #[test]
    fn rejects_present_but_malformed_optional_values() {
        let headers = create_test_headers();
        let record = csv::StringRecord::from(vec![
            "TicTacToe",
            "OpenAI",
            "model",
            "not-a-number",
            "",
            "",
            "Ollama",
            "model",
            "",
            "",
            "",
            "",
            "",
        ]);

        let error = CsvTestCase::from_record(record, &headers).unwrap_err();
        assert!(error.contains("Invalid agent_one_temp"));
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
