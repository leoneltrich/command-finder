use crate::ports::inbound::user_command::UserCommandPort;
use crate::core::errors::AppError;
use crate::core::models::EndUserConfig;

/// Dummy implementation of the Query Orchestrator.
/// Implements the UserCommandPort interface.
pub struct QueryOrchestrator;

impl QueryOrchestrator {
    /// Creates a new instance of the QueryOrchestrator.
    pub fn new() -> Self {
        Self
    }
}

impl Default for QueryOrchestrator {
    fn default() -> Self {
        Self::new()
    }
}

impl UserCommandPort for QueryOrchestrator {
    fn resolve_query(&self, raw_query: &str) -> Result<String, AppError> {
        Ok(format!("dummy_command_resolved_for: {}", raw_query))
    }

    fn update_configuration(&self, _config: &EndUserConfig) -> Result<bool, AppError> {
        Ok(true)
    }

    fn read_configuration(&self) -> Result<EndUserConfig, AppError> {
        Ok(EndUserConfig {
            logging_opt_in: false,
        })
    }
}
