use crate::core::errors::AppError;
use crate::core::models::{UserQuery, ScoredCandidate, ToolCatalog, OptimizedToolCatalog};

pub trait MatchingStrategyPort {
    fn calculate_similarities(
        &self,
        query: &UserQuery,
    ) -> Result<Vec<Vec<ScoredCandidate>>, AppError>;

    fn load_engines(&self) -> Result<bool, AppError>;

    fn optimize_catalog(
        &self,
        catalog: &ToolCatalog,
    ) -> Result<OptimizedToolCatalog, AppError>;
}
