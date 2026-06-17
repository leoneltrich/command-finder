use crate::ports::inbound::catalog_ingestion::CatalogIngestionPort;
use crate::ports::outbound::storage::StoragePort;
use crate::ports::outbound::matching_strategy::MatchingStrategyPort;
use crate::core::errors::AppError;
use crate::core::models::ToolCatalog;

/// Core interactor responsible for catalog lifecycle management.
/// Physically implements the CatalogIngestionPort inbound port.
pub struct CatalogLifecycleManager<S: StoragePort, M: MatchingStrategyPort> {
    storage_port: S,
    matching_port: M,
}

impl<S: StoragePort, M: MatchingStrategyPort> CatalogLifecycleManager<S, M> {
    /// Creates a new CatalogLifecycleManager instance.
    pub fn new(storage_port: S, matching_port: M) -> Self {
        Self { storage_port, matching_port }
    }
}

impl<S: StoragePort, M: MatchingStrategyPort> CatalogIngestionPort for CatalogLifecycleManager<S, M> {
    fn ingest_catalog(&self, catalog: &ToolCatalog, _auth_key: &str) -> Result<bool, AppError> {
        let optimized = self.matching_port.optimize_catalog(catalog)?;
        self.storage_port.save_catalog(&optimized)
    }

    fn update_catalog(&self, catalog: &ToolCatalog, _auth_key: &str) -> Result<bool, AppError> {
        let optimized = self.matching_port.optimize_catalog(catalog)?;
        self.storage_port.update_catalog(&optimized)
    }

    fn delete_catalog(&self, tool_name: &str, _auth_key: &str) -> Result<bool, AppError> {
        self.storage_port.delete_catalog(tool_name)
    }
}
