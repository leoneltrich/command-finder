use crate::ports::inbound::user_command::UserCommandPort;
use crate::ports::outbound::storage::StoragePort;
use crate::core::errors::AppError;
use crate::core::models::EndUserConfig;

/// Core interactor responsible for query resolution and user configuration.
pub struct QueryOrchestrator<S: StoragePort> {
    storage_port: S,
}

impl<S: StoragePort> QueryOrchestrator<S> {
    /// Creates a new instance of the QueryOrchestrator.
    pub fn new(storage_port: S) -> Self {
        Self { storage_port }
    }
}

impl<S: StoragePort> UserCommandPort for QueryOrchestrator<S> {
    fn resolve_query(&self, raw_query: &str) -> Result<String, AppError> {
        Ok(format!("dummy_command_resolved_for: {}", raw_query))
    }

    fn update_configuration(&self, config: &EndUserConfig) -> Result<bool, AppError> {
        self.storage_port.save_configuration(config)
    }

    fn read_configuration(&self) -> Result<EndUserConfig, AppError> {
        self.storage_port.load_configuration()
    }
}
