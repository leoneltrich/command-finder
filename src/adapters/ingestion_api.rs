use crate::ports::inbound::catalog_ingestion::CatalogIngestionPort;
use crate::core::errors::AppError;
use crate::core::models::ToolCatalog;

/// Driving adapter for the catalog ingestion interface.
/// It wraps an implementation of CatalogIngestionPort.
pub struct IngestionApi<P: CatalogIngestionPort> {
    catalog_ingestion_port: P,
}

impl<P: CatalogIngestionPort> IngestionApi<P> {
    /// Creates a new IngestionApi adapter.
    pub fn new(catalog_ingestion_port: P) -> Self {
        Self { catalog_ingestion_port }
    }

    /// Handles ingestion of a new tool catalog.
    pub fn ingest(&self, catalog: &ToolCatalog, auth_key: &str) -> Result<bool, AppError> {
        self.catalog_ingestion_port.ingest_catalog(catalog, auth_key)
    }

    /// Handles updating an existing tool catalog.
    pub fn update(&self, catalog: &ToolCatalog, auth_key: &str) -> Result<bool, AppError> {
        self.catalog_ingestion_port.update_catalog(catalog, auth_key)
    }

    /// Handles deleting a tool catalog.
    pub fn delete(&self, tool_name: &str, auth_key: &str) -> Result<bool, AppError> {
        self.catalog_ingestion_port.delete_catalog(tool_name, auth_key)
    }
}
