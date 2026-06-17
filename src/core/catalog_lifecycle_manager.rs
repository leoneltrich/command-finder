use crate::ports::inbound::catalog_ingestion::CatalogIngestionPort;
use crate::ports::outbound::storage::StoragePort;
use crate::core::errors::AppError;
use crate::core::models::{ToolCatalog, OptimizedToolCatalog, OptimizedCommandOption};

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
        let optimized = OptimizedToolCatalog {
            tool_name: catalog.tool_name.clone(),
            description: catalog.description.clone(),
            user_friendly_description: catalog.user_friendly_description.clone(),
            keywords: catalog.keywords.clone(),
            version: catalog.version.clone(),
            options: catalog.options.iter().map(|opt| OptimizedCommandOption {
                option_name: opt.option_name.clone(),
                description: opt.description.clone(),
                user_friendly_description: opt.user_friendly_description.clone(),
                keywords: opt.keywords.clone(),
                optimized_data: None,
            }).collect(),
            rules: catalog.rules.clone(),
            optimized_data: None,
        };
        self.storage_port.save_catalog(&optimized)
    }

    fn update_catalog(&self, catalog: &ToolCatalog, _auth_key: &str) -> Result<bool, AppError> {
        let optimized = OptimizedToolCatalog {
            tool_name: catalog.tool_name.clone(),
            description: catalog.description.clone(),
            user_friendly_description: catalog.user_friendly_description.clone(),
            keywords: catalog.keywords.clone(),
            version: catalog.version.clone(),
            options: catalog.options.iter().map(|opt| OptimizedCommandOption {
                option_name: opt.option_name.clone(),
                description: opt.description.clone(),
                user_friendly_description: opt.user_friendly_description.clone(),
                keywords: opt.keywords.clone(),
                optimized_data: None,
            }).collect(),
            rules: catalog.rules.clone(),
            optimized_data: None,
        };
        self.storage_port.update_catalog(&optimized)
    }

    fn delete_catalog(&self, tool_name: &str, _auth_key: &str) -> Result<bool, AppError> {
        self.storage_port.delete_catalog(tool_name)
    }
}
