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
use futures_util::FutureExt;
use games::{Game, GameEvent, print_game_stats};
use storage::{RunMetadata, RunStore};
use tokio::sync::mpsc;

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

#[tokio::main]
async fn main() {
    let args = ClapTestCase::parse();

    match args {
        ClapTestCase::Run(single_game_args) => {
            let game_name = single_game_args.game_name;
            let agent_one_config = AIAgentConfig {
                agent: single_game_args.agent_config.agent_one_kind,
                model: single_game_args.agent_config.agent_one_model,
                temp: single_game_args.agent_config.agent_one_temp,
                seed: Some(single_game_args.agent_config.agent_one_seed),
                secret_profile: single_game_args.agent_config.agent_one_secret_profile,
                api_endpoint: single_game_args.agent_config.agent_one_api_endpoint,
                prompt_file: single_game_args.agent_config.agent_one_prompt_file,
            };
            let agent_two_config = AIAgentConfig {
                agent: single_game_args.agent_config.agent_two_kind,
                model: single_game_args.agent_config.agent_two_model,
                temp: single_game_args.agent_config.agent_two_temp,
                seed: Some(single_game_args.agent_config.agent_two_seed),
                secret_profile: single_game_args.agent_config.agent_two_secret_profile,
                api_endpoint: single_game_args.agent_config.agent_two_api_endpoint,
                prompt_file: single_game_args.agent_config.agent_two_prompt_file,
            };

            let game = Game::from(game_name.as_str());
            let agents = vec![agent_one_config, agent_two_config];
            let repetitions = single_game_args.repetitions;
            let store = RunStore::new(single_game_args.db_path);
            if let Err(e) = store.initialize().await {
                eprintln!("Failed to initialize DuckDB run store: {}", e);
                std::process::exit(1);
            }

            println!(
                "Starting {} game ({} repetitions)...",
                game_name.as_str(),
                repetitions
            );

            for i in 0..repetitions {
                println!("Repetition {}/{}", i + 1, repetitions);
                let metadata = RunMetadata {
                    source: Some("cli".to_string()),
                    description: None,
                    repetition: Some(i + 1),
                    matrix_job_id: None,
                };
                let run_id = match store.create_run(&game, &agents, Some(metadata)).await {
                    Ok(run_id) => run_id,
                    Err(e) => {
                        eprintln!("Failed to create DuckDB run: {}", e);
                        std::process::exit(1);
                    }
                };
                if let Err(e) = store.mark_running(&run_id).await {
                    eprintln!("Failed to update DuckDB run {}: {}", run_id, e);
                    std::process::exit(1);
                }

                let result = if single_game_args.watch {
                    play_game_with_watch(game.clone(), agents.clone()).await
                } else {
                    std::panic::AssertUnwindSafe(game.play_game(agents.clone(), None, None))
                        .catch_unwind()
                        .await
                        .map_err(|_| "game execution panicked".to_string())
                };
                match result {
                    Ok(result) => {
                        if let Err(e) = store.complete_run(&run_id, &result).await {
                            eprintln!("Failed to store DuckDB run {}: {}", run_id, e);
                            std::process::exit(1);
                        }
                        print_game_stats(game_name.as_str(), &result);
                    }
                    Err(error) => {
                        let _ = store.mark_error(&run_id, &error).await;
                        eprintln!("Game failed: {}; run {} marked as error", error, run_id);
                        std::process::exit(1);
                    }
                }
            }
        }
        ClapTestCase::Batch(batch_args) => {
            if let Err(e) = run_csv_batch(&batch_args.test_file, true, batch_args.db_path).await {
                eprintln!("Error running CSV batch: {}", e);
                std::process::exit(1);
            }
        }
        ClapTestCase::Replay(replay_args) => {
            if let Err(e) = replay_run(replay_args).await {
                eprintln!("Error replaying game: {}", e);
                std::process::exit(1);
            }
        }
        ClapTestCase::Serve(serve_args) => {
            let config = server::ServerConfig {
                host: serve_args.host,
                port: serve_args.port,
                static_dir: serve_args.static_dir,
                db_path: serve_args.db_path,
                auth_token: serve_args.auth_token,
                form_defaults: server::FormDefaults {
                    game_type: serve_args.game_name,
                    agent_one: server::AgentFormDefaults {
                        kind: serve_args.agent_config.agent_one_kind,
                        model: serve_args.agent_config.agent_one_model,
                        temp: serve_args.agent_config.agent_one_temp,
                        seed: Some(serve_args.agent_config.agent_one_seed),
                        secret_profile: serve_args.agent_config.agent_one_secret_profile,
                        api_endpoint: serve_args.agent_config.agent_one_api_endpoint,
                        prompt_file: serve_args.agent_config.agent_one_prompt_file,
                    },
                    agent_two: server::AgentFormDefaults {
                        kind: serve_args.agent_config.agent_two_kind,
                        model: serve_args.agent_config.agent_two_model,
                        temp: serve_args.agent_config.agent_two_temp,
                        seed: Some(serve_args.agent_config.agent_two_seed),
                        secret_profile: serve_args.agent_config.agent_two_secret_profile,
                        api_endpoint: serve_args.agent_config.agent_two_api_endpoint,
                        prompt_file: serve_args.agent_config.agent_two_prompt_file,
                    },
                },
            };
            if let Err(e) = server::run_server(config).await {
                eprintln!("Server error: {}", e);
            }
        }
        ClapTestCase::Secrets(secrets_args) => {
            let mut secrets_manager = secrets::SecretsManager::load().unwrap_or_else(|e| {
                eprintln!("Failed to load secrets: {}", e);
                std::process::exit(1);
            });

            match secrets_args.command {
                SecretsCommand::Get { key } => match secrets_manager.get(&key) {
                    Some(value) => println!("{}", value),
                    None => {
                        eprintln!("Secret not found: {}", key);
                        std::process::exit(1);
                    }
                },
                SecretsCommand::Set { key, value } => {
                    if let Err(e) = secrets_manager.set(&key, &value) {
                        eprintln!("Failed to set secret: {}", e);
                        std::process::exit(1);
                    }
                    if let Err(e) = secrets_manager.save() {
                        eprintln!("Failed to save secrets: {}", e);
                        std::process::exit(1);
                    }
                    println!("Secret set: {}", key);
                }
                SecretsCommand::Encrypt => {
                    use std::io::Write;
                    print!("Enter password: ");
                    std::io::stdout().flush().unwrap();
                    let password = rpassword::read_password().unwrap();
                    print!("Confirm password: ");
                    std::io::stdout().flush().unwrap();
                    let confirm = rpassword::read_password().unwrap();

                    if password != confirm {
                        eprintln!("Passwords do not match");
                        std::process::exit(1);
                    }

                    secrets_manager.set_password(&password);
                    if let Err(e) = secrets_manager.save() {
                        eprintln!("Failed to save encrypted secrets: {}", e);
                        std::process::exit(1);
                    }
                    println!("Secrets file encrypted and saved.");
                }
            }
        }
    }
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
    }
}

fn print_live_event(event: &GameEvent) {
    match event {
        GameEvent::Turn {
            game_type,
            turn_number,
            state,
        } => {
            eprintln!("\n[{game_type} · turn {turn_number}]");
            eprintln!("{}", format_live_state(state));
        }
        GameEvent::Complete { game_type, .. } => {
            eprintln!("\n[{game_type} · complete]");
        }
        GameEvent::Error { game_type, message } => {
            eprintln!("\n[{game_type} · error] {message}");
        }
    }
}

fn format_live_state(state: &serde_json::Value) -> String {
    if let Some(board) = state.get("board").and_then(serde_json::Value::as_array) {
        let rows = board
            .iter()
            .map(|row| {
                row.as_array()
                    .map(|cells| {
                        cells
                            .iter()
                            .map(|cell| cell.as_str().unwrap_or("·"))
                            .collect::<Vec<_>>()
                            .join(" ")
                    })
                    .unwrap_or_else(|| row.to_string())
            })
            .collect::<Vec<_>>();
        return rows.join("\n");
    }

    if let Some(fen) = state.get("fen").and_then(serde_json::Value::as_str) {
        return format!("FEN: {fen}");
    }

    serde_json::to_string_pretty(state).unwrap_or_else(|_| state.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn live_board_formatting_is_human_readable() {
        assert_eq!(
            format_live_state(&json!({"board": [["X", null], [null, "O"]]})),
            "X ·\n· O"
        );
    }
}

#[derive(clap::Args, Debug)]
struct ServeArgs {
    #[clap(flatten)]
    agent_config: ClapAgentConfig,
    /// Initial game selected in the HTMX UI
    #[arg(long, default_value = "TicTacToe")]
    game_name: String,
    /// Host address to bind
    #[arg(long, default_value = "127.0.0.1")]
    host: String,
    /// Port to listen on
    #[arg(long, default_value_t = 3000)]
    port: u16,
    /// Optional directory of additional static assets
    #[arg(long)]
    static_dir: Option<std::path::PathBuf>,
    /// Path to the DuckDB file used for run history
    #[arg(long, default_value = "ai_arena.duckdb")]
    db_path: std::path::PathBuf,
    /// Require this bearer token for every server route except health checks
    #[arg(long, env = "AI_ARENA_AUTH_TOKEN")]
    auth_token: Option<String>,
}

#[derive(clap::Args, Debug)]
struct BatchArgs {
    /// Path to a CSV file containing multiple test cases
    #[arg(long)]
    test_file: String,
    /// Path to the DuckDB file used for run history
    #[arg(long, default_value = "ai_arena.duckdb")]
    db_path: std::path::PathBuf,
}

#[derive(clap::Args, Debug)]
struct ReplayArgs {
    /// Identifier of the persisted run to replay
    run_id: String,
    /// Path to the DuckDB file used for run history
    #[arg(long, default_value = "ai_arena.duckdb")]
    db_path: std::path::PathBuf,
}

async fn replay_run(args: ReplayArgs) -> Result<(), String> {
    let store = RunStore::new(args.db_path);
    store.initialize().await?;
    let run = store
        .get_run(&args.run_id)
        .await?
        .ok_or_else(|| format!("run {} was not found", args.run_id))?;
    let turns = store.list_turns(&args.run_id).await?;

    println!("Replaying {} ({})", run.game_type, run.id);
    println!("Status: {} · {} recorded turn(s)", run.status, turns.len());
    for event in turns {
        println!(
            "\nTurn {} · {} · {}",
            event.turn.turn_number, event.turn.player, event.turn.move_made
        );
        println!("{}", format_live_state(&event.turn.state_after));
        if let Some(error) = event.turn.error_message {
            println!("Move error: {error}");
        }
    }

    if let Some(result) = run.result {
        print_game_stats(&run.game_type, &result);
    }
    Ok(())
}
