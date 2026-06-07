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

        // 2. Load engines and calculate similarities
        let mut all_engine_results = Vec::new();
        for engine in &self.matching_engines {
            engine.load_engines()?;
            let similarities = engine.calculate_similarities(&user_query)?;
            all_engine_results.push(similarities);
        }

        // 3. Aggregate all results via the SimilarityRankAggregator
        let aggregated_results = self.rank_aggregator.aggregate(&all_engine_results)?;

        // Log the aggregated candidate base commands
        println!("[QueryOrchestrator] Aggregation finished. Top candidates resolved:");
        for (i, group) in aggregated_results.iter().enumerate() {
            println!("  Group {}:", i);
            for candidate in group {
                println!(
                    "    - Candidate option base '{}' (score: {})",
                    candidate.option.base, candidate.score
                );
            }
        }

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
