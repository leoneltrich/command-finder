use crate::core::errors::AppError;
use crate::core::models::{CommandItem, CommandCatalog};
use crate::ports::outbound::storage::StoragePort;

pub struct FileStorageAdapter;

impl StoragePort for FileStorageAdapter {
    fn save_catalog(&self, _catalog: &CommandCatalog) -> Result<(), AppError> {
        todo!()
    }

    fn load_all(&self) -> Result<Vec<CommandItem>, AppError> {
        todo!()
    }
}
