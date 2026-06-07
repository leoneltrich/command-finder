use crate::core::errors::AppError;
use crate::core::models::{UserQuery, ScoredCandidate};

pub trait MatchingStrategyPort {
    fn calculate_similarities(
        &self,
        query: &UserQuery,
    ) -> Result<Vec<Vec<ScoredCandidate>>, AppError>;

    fn load_engines(&self) -> Result<bool, AppError>;
}
