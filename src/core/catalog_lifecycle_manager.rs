use crate::ports::inbound::catalog_ingestion::CatalogIngestionPort;
use crate::ports::outbound::storage::StoragePort;
use crate::core::errors::AppError;
use crate::core::models::ToolCatalog;

/// Core interactor responsible for catalog lifecycle management.
/// Physically implements the CatalogIngestionPort inbound port.
pub struct CatalogLifecycleManager<S: StoragePort> {
    storage_port: S,
}

impl<S: StoragePort> CatalogLifecycleManager<S> {
    /// Creates a new CatalogLifecycleManager instance.
    pub fn new(storage_port: S) -> Self {
        Self { storage_port }
    }
}

impl<S: StoragePort> CatalogIngestionPort for CatalogLifecycleManager<S> {
    fn ingest_catalog(&self, catalog: &ToolCatalog, _auth_key: &str) -> Result<bool, AppError> {
        self.storage_port.save_catalog(catalog)
    }

    fn update_catalog(&self, catalog: &ToolCatalog, _auth_key: &str) -> Result<bool, AppError> {
        self.storage_port.update_catalog(catalog)
    }

    fn delete_catalog(&self, tool_name: &str, _auth_key: &str) -> Result<bool, AppError> {
        self.storage_port.delete_catalog(tool_name)
    }
}
