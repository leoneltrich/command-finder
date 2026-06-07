use crate::ports::inbound::user_command::UserCommandPort;
use crate::core::errors::AppError;
use crate::core::models::EndUserConfig;

/// Adapter that acts as the CLI controller for the application.
/// It receives commands/queries from the CLI and delegates them to the UserCommandPort.
pub struct CliController<P: UserCommandPort> {
    user_command_port: P,
}

impl<P: UserCommandPort> CliController<P> {
    /// Creates a new CliController wrapping a UserCommandPort implementation.
    pub fn new(user_command_port: P) -> Self {
        Self { user_command_port }
    }

    /// Handles a query request from the CLI.
    pub fn handle_query(&self, raw_query: &str) -> Result<String, AppError> {
        self.user_command_port.resolve_query(raw_query)
    }

    /// Reads the current configuration.
    pub fn read_config(&self) -> Result<EndUserConfig, AppError> {
        self.user_command_port.read_configuration()
    }

    /// Updates the configuration.
    pub fn update_config(&self, config: &EndUserConfig) -> Result<bool, AppError> {
        self.user_command_port.update_configuration(config)
    }
}
