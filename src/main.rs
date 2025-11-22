mod agent;
mod agents;
mod agent_config;
mod games;
mod csv_runner;
mod secrets;

use clap::Parser;
use games::{Game, print_game_stats};
use agent_config::{AIAgentConfig, AgentKind};
use csv_runner::run_csv_batch;

#[derive(Parser, Debug)]
#[command(name = "ai_arena")]
struct Args {
    #[clap(flatten)]
    test_case: Option<ClapTestCase>,
    #[arg(long, short = 'f')]
    test_file: Option<String>,
}

#[derive(Clone, Debug, clap::Args)]
struct ClapTestCase {
    #[clap(flatten)]
    agent_config: ClapAgentConfig,
    #[arg(long, short)]
    game_name: String,
    #[arg(long, short)]
    repetitions: u32,
}


#[derive(Clone, Debug, serde::Deserialize, clap::Args)]
pub struct ClapAgentConfig {
    #[arg(long)]
    agent_one_seed: u64,
    #[arg(long)]
    agent_two_seed: u64,
    #[arg(long)]
    agent_one_model: String,
    #[arg(long)]
    agent_one_temp: f32,
    #[arg(value_enum, long)]
    agent_one_kind: AgentKind,
    #[arg(long)]
    agent_one_secret_profile: Option<String>,
    #[arg(long)]
    agent_two_model: String,
    #[arg(long)]
    agent_two_temp: f32,
    #[arg(value_enum, long)]
    agent_two_kind: AgentKind,
    #[arg(long)]
    agent_two_secret_profile: Option<String>,
}


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

#[tokio::main]
async fn main() {
    let args = Args::parse();
    if let Some(test_file) = args.test_file {
        // Run CSV batch file
        if let Err(e) = run_csv_batch(&test_file, true).await {
            eprintln!("Error running CSV batch: {}", e);
            std::process::exit(1);
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

#[derive(Debug, serde::Deserialize)]
struct TestBatch {
    cases: Vec<TestCase>,
}

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
            agents: agents,
            repetitions: config.repetitions,
        }
    }
}



