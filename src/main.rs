mod agent;
mod agent_config;
mod agents;
mod csv_runner;
mod games;
mod secrets;
mod server;
mod storage;

use agent_config::{AIAgentConfig, AgentKind};
use clap::Parser;
use csv_runner::run_csv_batch;
use games::{Game, print_game_stats};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
enum ClapTestCase {
    /// Run a single game
    Run(SingleGameArgs),
    /// Run a batch of games from a CSV file
    Batch(BatchArgs),
    /// Replay a persisted game turn by turn
    Replay(ReplayArgs),
    /// Start the web server
    Serve(ServeArgs),
    /// Manage secrets
    Secrets(SecretsArgs),
}

#[derive(clap::Args, Debug)]
struct SecretsArgs {
    #[command(subcommand)]
    command: SecretsCommand,
}

#[derive(clap::Subcommand, Debug)]
enum SecretsCommand {
    /// Get a secret value
    Get { key: String },
    /// Set a secret value
    Set { key: String, value: String },
    /// Enable encryption for the secrets file
    Encrypt,
}

#[derive(clap::Args, Debug)]
struct SingleGameArgs {
    #[clap(flatten)]
    agent_config: ClapAgentConfig,
    #[arg(long, short)]
    game_name: String,
    #[arg(long, short)]
    repetitions: u32,
    /// Print each typed game event while the match is running
    #[arg(long)]
    watch: bool,
    /// Path to the DuckDB file used for run history
    #[arg(long, default_value = "ai_arena.duckdb")]
    db_path: std::path::PathBuf,
}

#[derive(Clone, Debug, serde::Deserialize, clap::Args)]
pub struct ClapAgentConfig {
    #[arg(long, default_value_t = 100)]
    agent_one_seed: u64,
    #[arg(long, default_value = "gpt-5.4")]
    agent_one_model: String,
    #[arg(long, default_value_t = 0.7)]
    agent_one_temp: f32,
    #[arg(value_enum, long, default_value_t = AgentKind::OpenAI )]
    agent_one_kind: AgentKind,
    #[arg(long, default_value = "openai.general")]
    agent_one_secret_profile: Option<String>,
    #[arg(long, default_value = "http://192.168.1.104:11434")]
    agent_one_api_endpoint: Option<String>,
    /// Optional file containing additional instructions for agent one
    #[arg(long)]
    agent_one_prompt_file: Option<String>,
    #[arg(long, default_value_t = 100)]
    agent_two_seed: u64,
    #[arg(long, default_value = "qwen3.6:latest")]
    agent_two_model: String,
    #[arg(long, default_value_t = 0.7)]
    agent_two_temp: f32,
    #[arg(value_enum, long, default_value_t = AgentKind::Ollama )]
    agent_two_kind: AgentKind,
    #[arg(long, default_value = "openai.general")]
    agent_two_secret_profile: Option<String>,
    #[arg(long, default_value = "http://192.168.1.104:11434")]
    agent_two_api_endpoint: Option<String>,
    /// Optional file containing additional instructions for agent two
    #[arg(long)]
    agent_two_prompt_file: Option<String>,
}

#[allow(dead_code)]
fn clap_agents_to_real_agents(agents: ClapAgentConfig) -> Vec<AIAgentConfig> {
    vec![
        AIAgentConfig {
            model: agents.agent_one_model,
            temp: agents.agent_one_temp,
            seed: Some(agents.agent_one_seed),
            agent: agents.agent_one_kind,
            secret_profile: agents.agent_one_secret_profile,
        },
        AIAgentConfig {
            model: agents.agent_two_model,
            temp: agents.agent_two_temp,
            seed: Some(agents.agent_two_seed),
            agent: agents.agent_two_kind,
            secret_profile: agents.agent_two_secret_profile,
        },
    ]
}

async fn play_game_with_watch(
    game: Game,
    agents: Vec<AIAgentConfig>,
) -> Result<games::TestResult, String> {
    let (sender, mut receiver) = mpsc::channel(64);
    let game_future =
        std::panic::AssertUnwindSafe(game.play_game(agents, Some(sender), None)).catch_unwind();
    tokio::pin!(game_future);

    loop {
        tokio::select! {
            event = receiver.recv() => {
                if let Some(event) = event {
                    print_live_event(&event);
                }
            }
            result = &mut game_future => {
                while let Ok(event) = receiver.try_recv() {
                    print_live_event(&event);
                }
                return result.map_err(|_| "game execution panicked".to_string());
            }
        }
    } else if let Some(test_case) = args.test_case {
        let case: TestCase = test_case.into();
        let game = case.game_name;
        let game_name = game.name();
        let result = game.play_game(case.agents.clone()).await;

        // Print formatted statistics
        print_game_stats(game_name, &result);
    } else {
        println!("No test case or test file provided.");
    }
}

#[allow(dead_code)]
#[derive(Clone, Debug, serde::Deserialize)]
struct TestCase {
    game_name: Game,
    description: String,
    agents: Vec<AIAgentConfig>,
    repetitions: u32,
}

impl From<ClapTestCase> for TestCase {
    fn from(config: ClapTestCase) -> Self {
        let agents = clap_agents_to_real_agents(config.agent_config);
        TestCase {
            // Need to make a new game here
            game_name: Game::from(config.game_name.as_str()),
            description: "manual run".to_string(),
            agents,
            repetitions: config.repetitions,
        }
    }
}
