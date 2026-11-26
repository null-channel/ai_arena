use std::{collections::HashMap, sync::Arc};
use tokio::sync::Mutex;

use super::{config::FormDefaults, jobs::MatrixJob, sessions::GameSession};
use crate::secrets::SecretsManager;
use crate::storage::RunStore;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum ArenaMode {
    #[default]
    Test,
    Real,
}

impl ArenaMode {
    pub fn run_label(self) -> &'static str {
        match self {
            Self::Test => "Test",
            Self::Real => "Saved",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::Test => "test",
            Self::Real => "real",
        }
    }
}

#[derive(Clone)]
pub(super) struct AppState {
    pub(super) games: Arc<Mutex<HashMap<String, GameSession>>>,
    pub(super) secrets: Arc<Mutex<SecretsManager>>,
    pub(super) form_defaults: Arc<Mutex<FormDefaults>>,
    pub(super) store: RunStore,
    pub(super) test_store: RunStore,
    real_mode: Arc<std::sync::atomic::AtomicBool>,
    pub(super) turing: super::turing::Turing,
    pub(super) toolbench: super::toolbench::Toolbench,
    pub(super) matrix_jobs: Arc<Mutex<HashMap<String, MatrixJob>>>,
    pub(super) auth_token: Option<String>,
}

impl AppState {
    pub fn mode(&self) -> ArenaMode {
        if self.real_mode.load(std::sync::atomic::Ordering::Relaxed) {
            ArenaMode::Real
        } else {
            ArenaMode::Test
        }
    }
    pub fn set_mode(&self, mode: ArenaMode) {
        self.real_mode.store(
            mode == ArenaMode::Real,
            std::sync::atomic::Ordering::Relaxed,
        );
    }
    pub fn result_store(&self, mode: ArenaMode) -> &RunStore {
        match mode {
            ArenaMode::Real => &self.store,
            ArenaMode::Test => &self.test_store,
        }
    }
    pub async fn game_store(&self, id: &str) -> &RunStore {
        let mode = self
            .games
            .lock()
            .await
            .get(id)
            .map(|game| game.mode)
            .unwrap_or(ArenaMode::Real);
        self.result_store(mode)
    }

    pub(super) fn with_store_and_auth(
        secrets: SecretsManager,
        form_defaults: FormDefaults,
        store: RunStore,
        auth_token: Option<String>,
    ) -> Self {
        Self {
            games: Arc::new(Mutex::new(HashMap::new())),
            secrets: Arc::new(Mutex::new(secrets)),
            form_defaults: Arc::new(Mutex::new(form_defaults)),
            store,
            test_store: RunStore::in_memory(),
            real_mode: Default::default(),
            turing: Default::default(),
            toolbench: Default::default(),
            matrix_jobs: Arc::new(Mutex::new(HashMap::new())),
            auth_token,
        }
    }
}
