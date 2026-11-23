use super::{config::ServerConfig, route::build_router, state::AppState};
use crate::{secrets::SecretsManager, storage::RunStore};
use std::net::SocketAddr;
use tokio::net::TcpListener;

// Initialize the server
pub async fn run_server(config: ServerConfig) -> Result<(), Box<dyn std::error::Error>> {
    let store = RunStore::new(config.db_path);
    store.initialize().await?;
    let state = AppState::with_store_and_auth(
        load_secrets_manager(),
        config.form_defaults,
        store,
        config.auth_token,
    );
    let app = build_router(state, config.static_dir.as_deref());

    let addr: SocketAddr = format!("{}:{}", config.host, config.port).parse()?;
    let listener = TcpListener::bind(addr).await?;
    let bound = listener.local_addr()?;
    println!("Server running on http://{}", bound);

    axum::serve(listener, app).await?;
    Ok(())
}

fn load_secrets_manager() -> SecretsManager {
    SecretsManager::load().unwrap_or_else(|e| {
        eprintln!("Warning: Failed to load secrets: {}", e);
        SecretsManager::load_from_path(std::path::Path::new("/dev/null")).unwrap()
    })
}
