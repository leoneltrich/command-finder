use crate::ports::outbound::matching_strategy::MatchingStrategyPort;
use crate::core::errors::AppError;
use crate::core::models::{UserQuery, ScoredCandidate, CommandOption, ToolCatalog, OptimizedToolCatalog, OptimizedCommandOption};

/// Outbound adapter representing the keyword-based matching engine.
#[derive(Clone, Copy)]
pub struct KeywordMatchingEngine;

impl KeywordMatchingEngine {
    /// Creates a new KeywordMatchingEngine instance.
    pub fn new() -> Self {
        Self
    }
}

impl Default for KeywordMatchingEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl MatchingStrategyPort for KeywordMatchingEngine {
    fn calculate_similarities(
        &self,
        query: &UserQuery,
    ) -> Result<Vec<Vec<ScoredCandidate>>, AppError> {
        // Return a dummy matched option from the keyword engine
        Ok(vec![vec![ScoredCandidate {
            option: CommandOption {
                option_name: "-la".to_string(),
                description: format!("Keyword match result for: {}", query.query),
                user_friendly_description: "".to_string(),
                keywords: "keyword".to_string(),
            },
            score: 0.85,
        }]])
    }

    fn load_engines(&self) -> Result<bool, AppError> {
        Ok(true)
    }

    fn optimize_catalog(
        &self,
        catalog: &ToolCatalog,
    ) -> Result<OptimizedToolCatalog, AppError> {
        let options = catalog.options.iter().map(OptimizedCommandOption::from).collect();

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
