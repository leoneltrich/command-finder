use crate::core::errors::AppError;
use crate::core::models::CommandCatalog;

pub trait CatalogIngestionUseCase {
    fn ingest_catalog(&self, catalog: CommandCatalog) -> Result<(), AppError>;
}
