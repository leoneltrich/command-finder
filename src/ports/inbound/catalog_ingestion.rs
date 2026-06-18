use crate::core::errors::AppError;
use crate::core::models::OptimizedToolCatalog;

/// Inbound port for managing tool catalogs (ingestion, updates, and deletion).
/// Corresponds to the specification in Table 8 of the design document.
pub trait CatalogIngestionPort {
    /// Ingests a new tool catalog.
    /// Returns `true` on success. Throws errors like `AuthenticationException` or `InvalidCatalogException` on failure.
    fn ingest_catalog(&self, catalog: &OptimizedToolCatalog, auth_key: &str) -> Result<bool, AppError>;

    /// Updates an existing tool catalog.
    /// Returns `true` on success. Throws errors like `CatalogNotFoundException` on failure.
    fn update_catalog(&self, catalog: &OptimizedToolCatalog, auth_key: &str) -> Result<bool, AppError>;

    /// Deletes a tool catalog by its tool name.
    /// Returns `true` on success. Throws errors like `CatalogNotFoundException` on failure.
    fn delete_catalog(&self, tool_name: &str, auth_key: &str) -> Result<bool, AppError>;
}
