use crate::core::errors::AppError;
use crate::core::models::{SearchQuery, SearchResult};

pub trait UserCommandUseCase {
    fn search_commands(&self, query: SearchQuery) -> Result<Vec<SearchResult>, AppError>;
}
