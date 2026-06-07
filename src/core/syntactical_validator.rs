use crate::core::models::{CommandObject, CommandRules};
use crate::core::errors::AppError;

/// Syntactical validator that parses command layouts and enforces exclusivity rules.
/// Currently operating as a dummy implementation.
pub struct SyntacticalValidator;

impl SyntacticalValidator {
    /// Creates a new SyntacticalValidator.
    pub fn new() -> Self {
        Self
    }

    /// Validates the command object against the rules, returning the final command string.
    pub fn validate(&self, command: &CommandObject, _rules: &CommandRules) -> Result<String, AppError> {
        // Dummy logic: join the base command with all detected options/flags
        let mut final_command = command.base_command.clone();
        for option in &command.options {
            final_command.push(' ');
            final_command.push_str(option);
        }
        Ok(final_command)
    }
}

impl Default for SyntacticalValidator {
    fn default() -> Self {
        Self::new()
    }
}
