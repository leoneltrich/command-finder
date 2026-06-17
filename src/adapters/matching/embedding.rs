use crate::ports::outbound::matching_strategy::MatchingStrategyPort;
use crate::core::errors::AppError;
use crate::core::models::{UserQuery, ScoredCandidate, CommandOption, ToolCatalog, OptimizedToolCatalog, OptimizedCommandOption};

/// Outbound adapter representing the embedding-based matching engine.
#[derive(Clone, Copy)]
pub struct EmbeddingMatchingEngine;

impl EmbeddingMatchingEngine {
    /// Creates a new EmbeddingMatchingEngine instance.
    pub fn new() -> Self {
        Self
    }
}

impl Default for EmbeddingMatchingEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl MatchingStrategyPort for EmbeddingMatchingEngine {
    fn calculate_similarities(
        &self,
        query: &UserQuery,
    ) -> Result<Vec<Vec<ScoredCandidate>>, AppError> {
        // Return a dummy matched option from the embedding engine
        Ok(vec![vec![ScoredCandidate {
            option: CommandOption {
                option_name: "-la".to_string(),
                description: format!("Embedding match result for: {}", query.query),
                user_friendly_description: "".to_string(),
                keywords: "embedding".to_string(),
            },
            score: 0.95,
        }]])
    }

    fn load_engines(&self) -> Result<bool, AppError> {
        Ok(true)
    }

    fn optimize_catalog(
        &self,
        catalog: &ToolCatalog,
    ) -> Result<OptimizedToolCatalog, AppError> {
        let options = catalog.options.iter().map(|opt| {
            OptimizedCommandOption {
                option_name: opt.option_name.clone(),
                description: opt.description.clone(),
                user_friendly_description: opt.user_friendly_description.clone(),
                keywords: opt.keywords.clone(),
                optimized_data: None,
            }
        }).collect();

        Ok(OptimizedToolCatalog {
            tool_name: catalog.tool_name.clone(),
            description: catalog.description.clone(),
            user_friendly_description: catalog.user_friendly_description.clone(),
            keywords: catalog.keywords.clone(),
            version: catalog.version.clone(),
            options,
            rules: catalog.rules.clone(),
            optimized_data: None,
        })
    }
}
