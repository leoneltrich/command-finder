use crate::core::errors::AppError;
use crate::core::models::{CommandItem, SearchQuery, SearchResult};
use crate::ports::outbound::matching_strategy::MatchingStrategyPort;

pub struct EmbeddingMatchingAdapter;

impl MatchingStrategyPort for EmbeddingMatchingAdapter {
    fn match_commands(&self, _query: &SearchQuery, _commands: &[CommandItem]) -> Result<Vec<SearchResult>, AppError> {
        todo!()
    }
}
