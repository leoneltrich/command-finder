pub mod catalog;
pub mod config;
pub mod maintainer;
pub mod search;

// Re-export all model types so external files can import them via crate::core::models::*
pub use catalog::{CommandCatalog, CommandOption, CommandRules};
pub use config::EndUserConfig;
pub use maintainer::CatalogMaintainer;
pub use search::{UserQuery, ScoredCandidate};