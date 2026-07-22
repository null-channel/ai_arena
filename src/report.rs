use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::agent_config::{AIAgentConfig, AgentKind};
use crate::games::TestResult;
use crate::games::stats::GameOutcome;

pub fn agents_for_repetition(agents: &[AIAgentConfig], repetition: u32) -> Vec<AIAgentConfig> {
    let mut ordered = agents.to_vec();
    if repetition.is_multiple_of(2) {
        ordered.reverse();
    }
    ordered
}

#[derive(Debug, Serialize)]
pub struct MatchRecord<'a> {
    pub schema_version: u32,
    pub prompt_protocol_version: u32,
    pub arena_version: &'static str,
    pub match_id: String,
    pub recorded_at_unix_ms: u128,
    pub game: &'a str,
    pub repetition: u32,
    pub total_repetitions: u32,
    pub agents: Vec<AgentMetadata<'a>>,
    pub result: &'a TestResult,
}

#[derive(Debug, Serialize)]
pub struct AgentMetadata<'a> {
    pub seat: usize,
    pub provider: AgentKind,
    pub model: &'a str,
    pub temperature: f32,
    pub requested_seed: Option<u64>,
    pub effective_seed: Option<u64>,
}

impl<'a> MatchRecord<'a> {
    pub fn new(
        game: &'a str,
        repetition: u32,
        total_repetitions: u32,
        agents: &'a [AIAgentConfig],
        result: &'a TestResult,
    ) -> Self {
        let recorded_at_unix_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let agents = agents
            .iter()
            .enumerate()
            .map(|(index, config)| AgentMetadata {
                seat: index + 1,
                provider: config.agent,
                model: &config.model,
                temperature: config.temp,
                requested_seed: config.seed,
                effective_seed: match config.agent {
                    AgentKind::OpenAI | AgentKind::Ollama => config.seed,
                    AgentKind::Anthropic => None,
                },
            })
            .collect();

        Self {
            schema_version: 1,
            prompt_protocol_version: 1,
            arena_version: env!("CARGO_PKG_VERSION"),
            match_id: uuid::Uuid::new_v4().to_string(),
            recorded_at_unix_ms,
            game,
            repetition,
            total_repetitions,
            agents,
            result,
        }
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct OutcomeSummary {
    total: u32,
    wins: BTreeMap<String, u32>,
    draws: u32,
    errors: u32,
}

impl OutcomeSummary {
    pub fn record(&mut self, outcome: &GameOutcome) {
        self.total += 1;
        match outcome {
            GameOutcome::Winner { winner } | GameOutcome::Forfeit { winner, .. } => {
                *self.wins.entry(winner.clone()).or_default() += 1;
            }
            GameOutcome::Draw => self.draws += 1,
            GameOutcome::Error { .. } | GameOutcome::InProgress => self.errors += 1,
        }
    }

    pub fn print(&self) {
        if self.total == 0 {
            return;
        }

        println!("Outcome summary:");
        for (winner, wins) in &self.wins {
            println!(
                "  {winner}: {wins} win(s) ({:.1}%)",
                percentage(*wins, self.total)
            );
        }
        println!(
            "  Draws: {} ({:.1}%)",
            self.draws,
            percentage(self.draws, self.total)
        );
        if self.errors > 0 {
            println!(
                "  Errors: {} ({:.1}%)",
                self.errors,
                percentage(self.errors, self.total)
            );
        }
    }
}

fn percentage(count: u32, total: u32) -> f64 {
    f64::from(count) * 100.0 / f64::from(total)
}

pub struct JsonlWriter {
    writer: BufWriter<File>,
}

impl JsonlWriter {
    pub fn create(path: impl AsRef<Path>) -> Result<Self, String> {
        let path = path.as_ref();
        let file = File::create(path)
            .map_err(|error| format!("failed to create {}: {error}", path.display()))?;
        Ok(Self {
            writer: BufWriter::new(file),
        })
    }

    pub fn write(&mut self, record: &MatchRecord<'_>) -> Result<(), String> {
        serde_json::to_writer(&mut self.writer, record)
            .map_err(|error| format!("failed to serialize match record: {error}"))?;
        self.writer
            .write_all(b"\n")
            .map_err(|error| format!("failed to write match record: {error}"))?;
        self.writer
            .flush()
            .map_err(|error| format!("failed to flush match record: {error}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::games::TicTacToeResult;
    use crate::games::stats::GameStats;

    #[test]
    fn reports_seat_order_and_effective_seed_support() {
        let agents = vec![
            AIAgentConfig {
                model: "claude".into(),
                temp: 0.2,
                seed: Some(10),
                agent: AgentKind::Anthropic,
                secret_profile: None,
            },
            AIAgentConfig {
                model: "gpt".into(),
                temp: 0.3,
                seed: Some(20),
                agent: AgentKind::OpenAI,
                secret_profile: None,
            },
        ];
        let result = TestResult::TicTacToe(TicTacToeResult {
            stats: GameStats::new(),
        });

        let value =
            serde_json::to_value(MatchRecord::new("TicTacToe", 1, 2, &agents, &result)).unwrap();

        assert_eq!(value["agents"][0]["seat"], 1);
        assert!(value["agents"][0]["effective_seed"].is_null());
        assert_eq!(value["agents"][1]["effective_seed"], 20);
        assert_eq!(value["repetition"], 1);
        assert_eq!(value["total_repetitions"], 2);
        assert_eq!(value["prompt_protocol_version"], 1);

        let reversed = agents_for_repetition(&agents, 2);
        assert_eq!(reversed[0].model, "gpt");
        assert_eq!(reversed[1].model, "claude");

        let path = std::env::temp_dir().join(format!("ai-arena-{}.jsonl", uuid::Uuid::new_v4()));
        {
            let mut writer = JsonlWriter::create(&path).unwrap();
            let record = MatchRecord::new("TicTacToe", 1, 2, &agents, &result);
            writer.write(&record).unwrap();
        }
        let contents = std::fs::read_to_string(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(contents.lines().count(), 1);
        assert!(serde_json::from_str::<serde_json::Value>(contents.trim()).is_ok());
    }

    #[test]
    fn aggregates_outcomes() {
        let mut summary = OutcomeSummary::default();
        summary.record(&GameOutcome::Winner {
            winner: "agent-a".into(),
        });
        summary.record(&GameOutcome::Forfeit {
            winner: "agent-a".into(),
            loser: "agent-b".into(),
            reason: "invalid moves".into(),
        });
        summary.record(&GameOutcome::Draw);
        summary.record(&GameOutcome::Error {
            message: "provider unavailable".into(),
        });

        assert_eq!(summary.total, 4);
        assert_eq!(summary.wins["agent-a"], 2);
        assert_eq!(summary.draws, 1);
        assert_eq!(summary.errors, 1);
    }
}
