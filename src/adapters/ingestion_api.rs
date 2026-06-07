use crate::ports::inbound::catalog_ingestion::CatalogIngestionUseCase;

pub struct IngestionApiAdapter<I>
where
    I: CatalogIngestionUseCase,
{
    ingestion_service: I,
}

impl<I> IngestionApiAdapter<I>
where
    I: CatalogIngestionUseCase,
{
    pub fn trigger_ingestion(&self) {
        todo!()
    }
}
