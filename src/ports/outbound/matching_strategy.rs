use crate::core::errors::AppError;
use crate::core::models::{CommandItem, SearchQuery, SearchResult};

pub trait MatchingStrategyPort {
    fn match_commands(&self, query: &SearchQuery, commands: &[CommandItem]) -> Result<Vec<SearchResult>, AppError>;
}
