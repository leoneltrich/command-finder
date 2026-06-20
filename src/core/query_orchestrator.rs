use crate::ports::inbound::user_command::UserCommandPort;
use crate::ports::outbound::storage::StoragePort;
use crate::ports::outbound::matching_strategy::MatchingStrategyPort;
use crate::core::errors::AppError;
use crate::core::models::EndUserConfig;
use crate::core::syntactical_validator::SyntacticalValidator;
use crate::core::similarity_rank_aggregator::SimilarityRankAggregator;

/// Core interactor responsible for query resolution and user configuration.
pub struct QueryOrchestrator<S: StoragePort> {
    storage_port: S,
    matching_engines: Vec<Box<dyn MatchingStrategyPort>>,
    validator: SyntacticalValidator,
    rank_aggregator: SimilarityRankAggregator,
}

impl<S: StoragePort> QueryOrchestrator<S> {
    /// Creates a new instance of the QueryOrchestrator.
    pub fn new(storage_port: S, matching_engines: Vec<Box<dyn MatchingStrategyPort>>) -> Self {
        Self {
            storage_port,
            matching_engines,
            validator: SyntacticalValidator::new(),
            rank_aggregator: SimilarityRankAggregator::new(),
        }
    }
}

impl<S: StoragePort> UserCommandPort for QueryOrchestrator<S> {
    fn resolve_query(&self, raw_query: &str) -> Result<String, AppError> {
        // 1. Construct UserQuery object
        let user_query = crate::core::models::UserQuery {
            query: raw_query.to_string(),
            n_grams: None,
        };

        // 2. Step 1: Find matching tools (catalogs)
        let mut all_tools = Vec::new();
        for engine in &self.matching_engines {
            engine.load_engines()?;
            let tools = engine.find_tools(&user_query)?;
            all_tools.extend(tools);
        }

        // Sort tools by score descending
        all_tools.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

        // Get the best tool name
        let matched_tool_name = if let Some(best_tool) = all_tools.first() {
            best_tool.tool.tool_name.clone()
        } else {
            "ls".to_string()
        };

        // Step 2: Find matching options for that tool
        let mut all_options = Vec::new();
        for engine in &self.matching_engines {
            let options = engine.find_options(&user_query, &matched_tool_name)?;
            all_options.extend(options);
        }

        // Sort options by score descending
        all_options.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

        // Log the aggregated candidate base commands
        println!("[QueryOrchestrator] Aggregation finished. Top tools resolved: {:?}", all_tools);
        println!("[QueryOrchestrator] Top option candidates resolved: {:?}", all_options);

        // Fetch default catalog definitions from storage
        let catalog = self.storage_port.fetch_catalog(&matched_tool_name)?;

        // Construct a command object matching detected query flags/options
        let base_command = catalog.tool_name.clone();
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
