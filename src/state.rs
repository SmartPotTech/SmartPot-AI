//! Estado compartido entre REST y gRPC.

use std::sync::Arc;

use crate::config::Settings;
use crate::engine::models::BaseModels;
use crate::learning::{Registry, TrainerClient};

#[derive(Clone)]
pub struct AppState {
    pub settings: Arc<Settings>,
    pub base: Arc<BaseModels>,
    pub learned: Arc<Registry>,
    pub trainer: Option<Arc<TrainerClient>>,
}

impl AppState {
    pub fn new(settings: Settings, base: BaseModels) -> Result<Self, String> {
        let trainer = match &settings.trainer_url {
            Some(url) => Some(Arc::new(TrainerClient::new(url, &settings.trainer_token)?)),
            None => None,
        };
        let learned = Arc::new(Registry::new(
            settings.models_dir.clone(),
            settings.trusted_key,
        ));
        Ok(Self {
            settings: Arc::new(settings),
            base: Arc::new(base),
            learned,
            trainer,
        })
    }
}
