use crate::ports::outbound::matching_strategy::MatchingStrategyPort;
use crate::core::errors::AppError;
use crate::core::models::{UserQuery, ScoredCandidate, CommandOption};

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
                intent: format!("Embedding match result for: {}", query.query),
                keywords: vec!["embedding".to_string()],
                option: "-la".to_string(),
            },
            score: 0.95,
        }]])
    }

    fn load_engines(&self) -> Result<bool, AppError> {
        Ok(true)
    }
}
