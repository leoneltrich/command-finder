use crate::ports::outbound::matching_strategy::MatchingStrategyPort;
use crate::core::errors::AppError;
use crate::core::models::{UserQuery, ScoredCandidate, CommandOption};

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
                intent: format!("Keyword match result for: {}", query.query),
                keywords: vec!["keyword".to_string()],
                base: "-la".to_string(),
            },
            score: 0.85,
        }]])
    }

    fn load_engines(&self) -> Result<bool, AppError> {
        Ok(true)
    }
}
