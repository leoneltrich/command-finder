use crate::ports::outbound::storage::StoragePort;
use crate::core::errors::AppError;
use crate::core::models::{ToolCatalog, CatalogMaintainer, EndUserConfig};

/// Persistence adapter implementing the outbound StoragePort.
pub struct PersistenceAdapter;

impl PersistenceAdapter {
    /// Creates a new PersistenceAdapter instance.
    pub fn new() -> Self {
        Self
    }
}

impl Default for PersistenceAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl StoragePort for PersistenceAdapter {
    // --- Catalog Management ---
    fn save_catalog(&self, _catalog: &ToolCatalog) -> Result<bool, AppError> {
        Ok(true)
    }

    fn update_catalog(&self, _catalog: &ToolCatalog) -> Result<bool, AppError> {
        Ok(true)
    }

    fn delete_catalog(&self, _tool_name: &str) -> Result<bool, AppError> {
        Ok(true)
    }

    fn fetch_catalog(&self, tool_name: &str) -> Result<ToolCatalog, AppError> {
        Ok(ToolCatalog {
            tool_name: tool_name.to_string(),
            description: "Dummy Catalog".to_string(),
            keywords: vec![],
            version: "0.1.0".to_string(),
            options: vec![],
            rules: crate::core::models::catalog::CommandRules {
                rules: "".to_string(),
            },
        })
    }

    fn fetch_all_catalogs(&self) -> Result<Vec<ToolCatalog>, AppError> {
        Ok(vec![])
    }

    // --- Authentication / Maintainer Data ---
    fn save_maintainer(&self, _maintainer: &CatalogMaintainer) -> Result<bool, AppError> {
        Ok(true)
    }

    fn update_maintainer(&self, _maintainer: &CatalogMaintainer) -> Result<bool, AppError> {
        Ok(true)
    }

    fn fetch_maintainer(&self, maintainer_id: &str) -> Result<CatalogMaintainer, AppError> {
        Ok(CatalogMaintainer {
            id: maintainer_id.to_string(),
            name: "Dummy Name".to_string(),
            auth_key: "dummy_auth_key".to_string(),
        })
    }

    fn delete_maintainer(&self, _maintainer_id: &str) -> Result<bool, AppError> {
        Ok(true)
    }

    // --- User Configuration ---
    fn load_configuration(&self) -> Result<EndUserConfig, AppError> {
        Ok(EndUserConfig {
            logging_opt_in: false,
        })
    }

    fn save_configuration(&self, _config: &EndUserConfig) -> Result<bool, AppError> {
        Ok(true)
    }
}
