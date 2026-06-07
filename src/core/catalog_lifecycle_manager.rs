use crate::ports::inbound::catalog_ingestion::CatalogIngestionPort;
use crate::core::errors::AppError;
use crate::core::models::ToolCatalog;

/// Core interactor responsible for catalog lifecycle management.
/// Physically implements the CatalogIngestionPort inbound port.
pub struct CatalogLifecycleManager;

impl CatalogLifecycleManager {
    /// Creates a new CatalogLifecycleManager instance.
    pub fn new() -> Self {
        Self
    }
}

impl Default for CatalogLifecycleManager {
    fn default() -> Self {
        Self::new()
    }
}

impl CatalogIngestionPort for CatalogLifecycleManager {
    fn ingest_catalog(&self, _catalog: &ToolCatalog, _auth_key: &str) -> Result<bool, AppError> {
        Ok(true)
    }

    fn update_catalog(&self, _catalog: &ToolCatalog, _auth_key: &str) -> Result<bool, AppError> {
        Ok(true)
    }

    fn delete_catalog(&self, _tool_name: &str, _auth_key: &str) -> Result<bool, AppError> {
        Ok(true)
    }
}
