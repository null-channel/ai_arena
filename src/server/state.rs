use std::{collections::HashMap, sync::Arc};
use tokio::sync::Mutex;

use super::{config::FormDefaults, jobs::MatrixJob, sessions::GameSession};
use crate::secrets::SecretsManager;
use crate::storage::RunStore;

#[derive(Clone)]
pub(super) struct AppState {
    pub(super) games: Arc<Mutex<HashMap<String, GameSession>>>,
    pub(super) secrets: Arc<Mutex<SecretsManager>>,
    pub(super) form_defaults: Arc<Mutex<FormDefaults>>,
    pub(super) store: RunStore,
    pub(super) turing: super::turing::Turing,
    pub(super) toolbench: super::toolbench::Toolbench,
    pub(super) matrix_jobs: Arc<Mutex<HashMap<String, MatrixJob>>>,
    pub(super) auth_token: Option<String>,
}

impl AppState {
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
            turing: Default::default(),
            toolbench: Default::default(),
            matrix_jobs: Arc::new(Mutex::new(HashMap::new())),
            auth_token,
        }
    }
}
