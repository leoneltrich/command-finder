use crate::core::errors::AppError;
use crate::core::models::{CommandCatalog, CatalogMaintainer, EndUserConfig};

pub trait StoragePort {
    // --- Catalog Management ---
    fn save_catalog(&self, catalog: &CommandCatalog) -> Result<bool, AppError>;
    fn update_catalog(&self, catalog: &CommandCatalog) -> Result<bool, AppError>;
    fn delete_catalog(&self, tool_name: &str) -> Result<bool, AppError>;
    fn fetch_catalog(&self, tool_name: &str) -> Result<CommandCatalog, AppError>;
    fn fetch_all_catalogs(&self) -> Result<Vec<CommandCatalog>, AppError>;

    // --- Authentication / Maintainer Data ---
    fn save_maintainer(&self, maintainer: &CatalogMaintainer) -> Result<bool, AppError>;
    fn update_maintainer(&self, maintainer: &CatalogMaintainer) -> Result<bool, AppError>;
    fn fetch_maintainer(&self, maintainer_id: &str) -> Result<CatalogMaintainer, AppError>;
    fn delete_maintainer(&self, maintainer_id: &str) -> Result<bool, AppError>;

    // --- User Configuration ---
    fn load_configuration(&self) -> Result<EndUserConfig, AppError>;
    fn save_configuration(&self, config: &EndUserConfig) -> Result<bool, AppError>;
}
