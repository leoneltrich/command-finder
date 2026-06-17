use crate::ports::outbound::matching_strategy::MatchingStrategyPort;
use crate::core::errors::AppError;
use crate::core::models::{UserQuery, ScoredCandidate, CommandOption, ToolCatalog, OptimizedToolCatalog, OptimizedCommandOption};
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::LlamaModel;

/// Outbound adapter representing the embedding-based matching engine.
#[derive(Clone, Copy)]
pub struct EmbeddingMatchingEngine;

impl EmbeddingMatchingEngine {
    /// Creates a new EmbeddingMatchingEngine instance.
    pub fn new() -> Self {
        Self
    }
}

impl Default for EmbeddingMatchingEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl MatchingStrategyPort for EmbeddingMatchingEngine {
    fn calculate_similarities(
        &self,
        query: &UserQuery,
    ) -> Result<Vec<Vec<ScoredCandidate>>, AppError> {
        // Return a dummy matched option from the embedding engine
        Ok(vec![vec![ScoredCandidate {
            option: CommandOption {
                option_name: "-la".to_string(),
                description: format!("Embedding match result for: {}", query.query),
                user_friendly_description: "".to_string(),
                keywords: "embedding".to_string(),
            },
            score: 0.95,
        }]])
    }

    fn load_engines(&self) -> Result<bool, AppError> {
        Ok(true)
    }

    fn optimize_catalog(
        &self,
        catalog: &ToolCatalog,
    ) -> Result<OptimizedToolCatalog, AppError> {
        let num_cpus = num_cpus::get_physical();
        let model_path = "/home/sandbox-noadmin/RustroverProjects/embedding_models_testing/models/embeddinggemma-300M-BF16.gguf"; //TODO Make dynamic

        if !std::path::Path::new(model_path).exists() {
            return Err(AppError::Initialization(
                crate::core::errors::InitializationException::new(format!(
                    "Model file does not exist at {}",
                    model_path
                )),
            ));
        }

        let backend = LlamaBackend::init()
            .map_err(|e| AppError::Initialization(
                crate::core::errors::InitializationException::new(format!(
                    "Failed to initialize llama-cpp backend: {:?}",
                    e
                )),
            ))?;

        let model_params = LlamaModelParams::default();
        let model = LlamaModel::load_from_file(&backend, model_path, &model_params)
            .map_err(|e| AppError::Initialization(
                crate::core::errors::InitializationException::new(format!(
                    "Failed to load model: {:?}",
                    e
                )),
            ))?;

        let ctx_params = LlamaContextParams::default()
            .with_embeddings(true)
            .with_n_ctx(std::num::NonZeroU32::new(512))
            .with_n_threads(num_cpus as i32)
            .with_n_threads_batch(num_cpus as i32);

        let _ctx = model.new_context(&backend, ctx_params)
            .map_err(|e| AppError::Initialization(
                crate::core::errors::InitializationException::new(format!(
                    "Failed to create context: {:?}",
                    e
                )),
            ))?;

        let options = catalog.options.iter().map(|opt| {
            OptimizedCommandOption {
                option_name: opt.option_name.clone(),
                description: opt.description.clone(),
                user_friendly_description: opt.user_friendly_description.clone(),
                keywords: opt.keywords.clone(),
                optimized_data: None,
            }
        }).collect();

        Ok(OptimizedToolCatalog {
            tool_name: catalog.tool_name.clone(),
            description: catalog.description.clone(),
            user_friendly_description: catalog.user_friendly_description.clone(),
            keywords: catalog.keywords.clone(),
            version: catalog.version.clone(),
            options,
            rules: catalog.rules.clone(),
            optimized_data: None,
        })
    }
}
