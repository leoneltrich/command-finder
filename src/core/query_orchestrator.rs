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

        // 2. Find matching tools (catalogs)
        let mut all_tools = Vec::new();
        for engine in &self.matching_engines {
            engine.load_engines()?;
            let tools = engine.find_tools(&user_query)?;
            all_tools.extend(tools);
        }

        // Sort tools by score descending
        all_tools.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

        // Format tool matches with all details
        let mut output = String::new();
        output.push_str("Tool matching results:\n");
        for (i, tool) in all_tools.iter().enumerate() {
            output.push_str(&format!(
                "  [{}] Name: {} (Score: {:.4})\n      Description: {}\n      Keywords: {}\n      Version: {}\n      Rules: {:?}\n",
                i + 1,
                tool.tool.tool_name,
                tool.score,
                tool.tool.description,
                tool.tool.keywords,
                tool.tool.version,
                tool.tool.rules
            ));
        }

        Ok(output)
    }

    fn update_configuration(&self, config: &EndUserConfig) -> Result<bool, AppError> {
        self.storage_port.save_configuration(config)
    }

    fn read_configuration(&self) -> Result<EndUserConfig, AppError> {
        self.storage_port.load_configuration()
    }
}
