use crate::core::errors::AppError;
use crate::core::models::{CommandItem, CommandCatalog};

pub trait StoragePort {
    fn save_catalog(&self, catalog: &CommandCatalog) -> Result<(), AppError>;
    fn load_all(&self) -> Result<Vec<CommandItem>, AppError>;
}
