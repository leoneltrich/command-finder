use crate::ports::inbound::catalog_ingestion::CatalogIngestionPort;
use crate::ports::outbound::storage::StoragePort;
use crate::ports::outbound::matching_strategy::MatchingStrategyPort;
use crate::core::errors::AppError;
use crate::core::models::ToolCatalog;

/// Core interactor responsible for catalog lifecycle management.
/// Physically implements the CatalogIngestionPort inbound port.
pub struct CatalogLifecycleManager<S: StoragePort> {
    storage_port: S,
    matching_engines: Vec<Box<dyn MatchingStrategyPort>>,
}

impl<S: StoragePort> CatalogLifecycleManager<S> {
    /// Creates a new CatalogLifecycleManager instance.
    pub fn new(storage_port: S, matching_engines: Vec<Box<dyn MatchingStrategyPort>>) -> Self {
        Self { storage_port, matching_engines }
    }
}

impl<S: StoragePort> CatalogIngestionPort for CatalogLifecycleManager<S> {
    fn ingest_catalog(&self, catalog: &ToolCatalog, _auth_key: &str) -> Result<bool, AppError> {
        self.storage_port.save_catalog(catalog)?;
        for engine in &self.matching_engines {
            engine.create_optimized_catalog(catalog)?;
        }
        Ok(true)
    }

    fn update_catalog(&self, catalog: &ToolCatalog, _auth_key: &str) -> Result<bool, AppError> {
        self.storage_port.update_catalog(catalog)?;
        for engine in &self.matching_engines {
            engine.update_optimized_catalog(catalog)?;
        }
        Ok(true)
    }

    fn delete_catalog(&self, tool_name: &str, _auth_key: &str) -> Result<bool, AppError> {
        self.storage_port.delete_catalog(tool_name)?;
        for engine in &self.matching_engines {
            engine.delete_optimized_catalog(tool_name)?;
        }
        Ok(true)
    }
}
