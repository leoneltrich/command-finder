use crate::ports::inbound::user_command::UserCommandPort;
use crate::ports::outbound::storage::StoragePort;
use crate::core::errors::AppError;
use crate::core::models::EndUserConfig;
use crate::core::syntactical_validator::SyntacticalValidator;

/// Core interactor responsible for query resolution and user configuration.
pub struct QueryOrchestrator<S: StoragePort> {
    storage_port: S,
    validator: SyntacticalValidator,
}

impl<S: StoragePort> QueryOrchestrator<S> {
    /// Creates a new instance of the QueryOrchestrator.
    pub fn new(storage_port: S) -> Self {
        Self {
            storage_port,
            validator: SyntacticalValidator::new(),
        }
    }
}

impl<S: StoragePort> UserCommandPort for QueryOrchestrator<S> {
    fn resolve_query(&self, raw_query: &str) -> Result<String, AppError> {
        // Fetch default catalog definitions from storage
        let catalog = self.storage_port.fetch_catalog("ls")?;

        // Construct a command object matching detected query flags/options
        let base_command = catalog.tool_name;
        let mut options = Vec::new();
        for word in raw_query.split_whitespace() {
            if word.starts_with('-') {
                options.push(word.to_string());
            }
        }
        if options.is_empty() {
            options.push("-la".to_string());
        }

        let command_object = crate::core::models::CommandObject {
            base_command,
            options,
        };

        // Validate options against catalog rules and build final command
        self.validator.validate(&command_object, &catalog.rules)
    }

    fn update_configuration(&self, config: &EndUserConfig) -> Result<bool, AppError> {
        self.storage_port.save_configuration(config)
    }

    fn read_configuration(&self) -> Result<EndUserConfig, AppError> {
        self.storage_port.load_configuration()
    }
}
