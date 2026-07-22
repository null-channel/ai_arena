mod agent;
mod agent_config;
mod agents;
mod csv_runner;
mod games;
mod report;
mod secrets;

use agent_config::{AIAgentConfig, AgentKind};
use clap::{ArgGroup, CommandFactory, Parser, error::ErrorKind};
use csv_runner::run_csv_batch;
use games::{Game, print_game_stats};
use report::{JsonlWriter, MatchRecord, OutcomeSummary, agents_for_repetition};

#[derive(Parser, Debug)]
#[command(
    name = "ai_arena",
    group(ArgGroup::new("input").required(true).multiple(false).args(["test_file", "game_name"]))
)]
struct Args {
    #[arg(long, short = 'f')]
    test_file: Option<String>,
    /// Write one machine-readable match record per line.
    #[arg(long)]
    output_jsonl: Option<String>,
    #[clap(flatten)]
    agent_config: ClapAgentConfig,
    #[arg(
        long,
        short,
        value_parser = ["TicTacToe", "RockPaperScissors", "ConnectFour"]
    )]
    game_name: Option<String>,
    #[arg(long, short, default_value_t = 1, value_parser = clap::value_parser!(u32).range(1..))]
    repetitions: u32,
}

#[derive(Clone, Debug, serde::Deserialize, clap::Args)]
pub struct ClapAgentConfig {
    #[arg(long, default_value_t = 0)]
    agent_one_seed: u64,
    #[arg(long, default_value_t = 0)]
    agent_two_seed: u64,
    #[arg(long)]
    agent_one_model: Option<String>,
    #[arg(long, default_value_t = 0.7, value_parser = parse_temperature)]
    agent_one_temp: f32,
    #[arg(value_enum, long)]
    agent_one_kind: Option<AgentKind>,
    #[arg(long)]
    agent_one_secret_profile: Option<String>,
    #[arg(long)]
    agent_two_model: Option<String>,
    #[arg(long, default_value_t = 0.7, value_parser = parse_temperature)]
    agent_two_temp: f32,
    #[arg(value_enum, long)]
    agent_two_kind: Option<AgentKind>,
    #[arg(long)]
    agent_two_secret_profile: Option<String>,
}

fn parse_temperature(value: &str) -> Result<f32, String> {
    let temperature = value
        .parse::<f32>()
        .map_err(|error| format!("invalid temperature: {error}"))?;
    if temperature.is_finite() && (0.0..=2.0).contains(&temperature) {
        Ok(temperature)
    } else {
        Err("temperature must be finite and between 0.0 and 2.0".into())
    }
}

fn clap_agents_to_real_agents(agents: ClapAgentConfig) -> Result<Vec<AIAgentConfig>, String> {
    Ok(vec![
        AIAgentConfig {
            model: agents
                .agent_one_model
                .ok_or("--agent-one-model is required for a manual game")?,
            temp: agents.agent_one_temp,
            seed: Some(agents.agent_one_seed),
            agent: agents
                .agent_one_kind
                .ok_or("--agent-one-kind is required for a manual game")?,
            secret_profile: agents.agent_one_secret_profile,
        },
        AIAgentConfig {
            model: agents
                .agent_two_model
                .ok_or("--agent-two-model is required for a manual game")?,
            temp: agents.agent_two_temp,
            seed: Some(agents.agent_two_seed),
            agent: agents
                .agent_two_kind
                .ok_or("--agent-two-kind is required for a manual game")?,
            secret_profile: agents.agent_two_secret_profile,
        },
    ])
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    let mut output = match args
        .output_jsonl
        .as_deref()
        .map(JsonlWriter::create)
        .transpose()
    {
        Ok(output) => output,
        Err(error) => {
            eprintln!("Error creating JSONL output: {error}");
            std::process::exit(1);
        }
    };
    if let Some(test_file) = args.test_file {
        // Run CSV batch file
        if let Err(e) = run_csv_batch(&test_file, true, output.as_mut()).await {
            eprintln!("Error running CSV batch: {}", e);
            std::process::exit(1);
        }
    } else if let Some(game_name_arg) = args.game_name {
        let agents = clap_agents_to_real_agents(args.agent_config).unwrap_or_else(|message| {
            Args::command()
                .error(ErrorKind::MissingRequiredArgument, message)
                .exit()
        });
        let game = Game::try_from(game_name_arg.as_str()).expect("validated by clap");
        let game_name = game.name();
        let mut summary = OutcomeSummary::default();
        for repetition in 1..=args.repetitions {
            if args.repetitions > 1 {
                println!("\n--- Repetition {repetition} of {} ---", args.repetitions);
            }
            let match_agents = agents_for_repetition(&agents, repetition);
            let result = game.play_game(match_agents.clone()).await;
            summary.record(&result.stats().outcome);
            print_game_stats(game_name, &result);
            if let Some(writer) = output.as_mut() {
                let record = MatchRecord::new(
                    game_name,
                    repetition,
                    args.repetitions,
                    &match_agents,
                    &result,
                );
                if let Err(error) = writer.write(&record) {
                    eprintln!("Error writing JSONL output: {error}");
                    std::process::exit(1);
                }
            }
        }
        if args.repetitions > 1 {
            summary.print();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manual_args(repetitions: &str) -> Vec<&str> {
        vec![
            "ai_arena",
            "--game-name",
            "TicTacToe",
            "--agent-one-kind",
            "open-ai",
            "--agent-one-model",
            "model-one",
            "--agent-two-kind",
            "ollama",
            "--agent-two-model",
            "model-two",
            "--repetitions",
            repetitions,
        ]
    }

    #[test]
    fn rejects_conflicting_input_modes() {
        let result = Args::try_parse_from([
            "ai_arena",
            "--test-file",
            "batch.csv",
            "--game-name",
            "TicTacToe",
        ]);
        assert!(result.is_err());
    }

    #[test]
    fn rejects_zero_repetitions() {
        assert!(parse_temperature("NaN").is_err());
        assert!(Args::try_parse_from(manual_args("0")).is_err());
    }

    #[test]
    fn accepts_batch_mode_without_manual_agent_arguments() {
        let args = Args::try_parse_from(["ai_arena", "--test-file", "batch.csv"]).unwrap();
        assert_eq!(args.test_file.as_deref(), Some("batch.csv"));
        assert!(args.game_name.is_none());
    }

    #[test]
    fn applies_manual_defaults() {
        let args = Args::try_parse_from(manual_args("2")).unwrap();
        assert_eq!(args.repetitions, 2);
        assert_eq!(args.agent_config.agent_one_temp, 0.7);
        assert_eq!(args.agent_config.agent_two_seed, 0);
    }
}
