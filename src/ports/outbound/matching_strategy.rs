use crate::core::errors::AppError;
use crate::core::models::{ScoredCandidate, ScoredTool, ToolCatalog, UserQuery};

pub trait MatchingStrategyPort {
    fn find_tools(&self, query: &UserQuery) -> Result<Vec<ScoredTool>, AppError>;

    fn find_options(
        &self,
        query: &UserQuery,
        tool_name: &str,
    ) -> Result<Vec<ScoredCandidate>, AppError>;

    fn load_engine(&self) -> Result<bool, AppError>;

    fn create_optimized_catalog(&self, catalog: &ToolCatalog) -> Result<(), AppError>;

    fn update_optimized_catalog(&self, catalog: &ToolCatalog) -> Result<(), AppError>;

    fn delete_optimized_catalog(&self, tool_name: &str) -> Result<(), AppError>;

    /// Gets the engine weight for tool retrieval.
    fn tool_weight(&self) -> f64;

    /// Gets the engine weight for option retrieval.
    fn option_weight(&self) -> f64;
}
