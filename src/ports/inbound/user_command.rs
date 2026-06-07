use crate::core::errors::AppError;
use crate::core::models::EndUserConfig;

/// Inbound port for handling end-user commands and queries.
pub trait UserCommandPort {
    /// Translates a user's natural language request into an executable command.
    /// If similarity scores fall below the minimum threshold, or if the resulting
    /// command violates catalog exclusivity rules, throws a `DisambiguationRequired` error.
    fn resolve_query(&self, raw_query: &str) -> Result<String, AppError>;

    /// Updates the system configuration.
    fn update_configuration(&self, config: &EndUserConfig) -> Result<bool, AppError>;

    /// Reads the current system configuration.
    fn read_configuration(&self) -> Result<EndUserConfig, AppError>;
}
