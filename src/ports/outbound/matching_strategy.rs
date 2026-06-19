use crate::core::errors::AppError;
use crate::core::models::{UserQuery, ScoredCandidate, ToolCatalog};

pub trait MatchingStrategyPort {
    fn calculate_similarities(
        &self,
        query: &UserQuery,
    ) -> Result<Vec<Vec<ScoredCandidate>>, AppError>;

    fn load_engines(&self) -> Result<bool, AppError>;

    fn create_optimized_catalog(
        &self,
        catalog: &ToolCatalog,
    ) -> Result<(), AppError>;

    fn update_optimized_catalog(
        &self,
        catalog: &ToolCatalog,
    ) -> Result<(), AppError>;

    fn delete_optimized_catalog(
        &self,
        tool_name: &str,
    ) -> Result<(), AppError>;
}
