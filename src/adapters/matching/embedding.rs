use crate::ports::outbound::matching_strategy::MatchingStrategyPort;
use crate::core::errors::AppError;
use crate::core::models::{UserQuery, ScoredCandidate, CommandOption, ToolCatalog, OptimizedToolCatalog, OptimizedCommandOption, OptimizedData};
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
        let model_path = "/home/sandbox-noadmin/RustroverProjects/embedding_models_testing/models/embeddinggemma-300M-BF16.gguf";

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

        let mut ctx = model.new_context(&backend, ctx_params)
            .map_err(|e| AppError::Initialization(
                crate::core::errors::InitializationException::new(format!(
                    "Failed to create context: {:?}",
                    e
                )),
            ))?;

        // Format the tool description using the standard gemma pattern
        let processed_text = format!("title: {} | text: {}", catalog.tool_name, catalog.description);

        let raw_emb = compute_embedding(&model, &mut ctx, &processed_text)?;
        let normalized_emb = l2_normalize(raw_emb);
        let data_bytes = serialize_embedding(&normalized_emb);

        let optimized_data = Some(OptimizedData {
            key: "gemma_embedding".to_string(),
            data: data_bytes,
            data_type: "BLOB".to_string(),
        });

        let options = catalog.options.iter().map(OptimizedCommandOption::from).collect();

        Ok(OptimizedToolCatalog {
            tool_name: catalog.tool_name.clone(),
            description: catalog.description.clone(),
            user_friendly_description: catalog.user_friendly_description.clone(),
            keywords: catalog.keywords.clone(),
            version: catalog.version.clone(),
            options,
            rules: catalog.rules.clone(),
            optimized_data,
        })
    }
}

// --- Private Helper Functions for Embedding Ingestion ---

fn compute_embedding(
    model: &LlamaModel,
    ctx: &mut llama_cpp_2::context::LlamaContext,
    text: &str,
) -> Result<Vec<f32>, AppError> {
    let tokens = model.str_to_token(text, llama_cpp_2::model::AddBos::Always)
        .map_err(|e| AppError::EngineExecution(
            crate::core::errors::EngineExecutionException::new(format!(
                "Tokenization failed: {:?}",
                e
            )),
        ))?;

    if tokens.is_empty() {
        return Err(AppError::EngineExecution(
            crate::core::errors::EngineExecutionException::new("Tokenization returned empty tokens"),
        ));
    }

    let token_count = tokens.len();
    let mut batch = llama_cpp_2::llama_batch::LlamaBatch::new(token_count, 1);
    for (i, &token) in tokens.iter().enumerate() {
        batch.add(token, i as i32, &[0], true)
            .map_err(|e| AppError::EngineExecution(
                crate::core::errors::EngineExecutionException::new(format!(
                    "Failed to add token to batch: {:?}",
                    e
                )),
            ))?;
    }

    ctx.decode(&mut batch)
        .map_err(|e| AppError::EngineExecution(
            crate::core::errors::EngineExecutionException::new(format!(
                "Decoding failed: {:?}",
                e
            )),
        ))?;

    let emb = ctx.embeddings_seq_ith(0)
        .map_err(|e| AppError::EngineExecution(
            crate::core::errors::EngineExecutionException::new(format!(
                "Failed to retrieve embeddings: {:?}",
                e
            )),
        ))?
        .to_vec();

    Ok(emb)
}

fn l2_normalize(mut emb: Vec<f32>) -> Vec<f32> {
    let mut sum_sq = 0.0;
    for &val in &emb {
        sum_sq += val * val;
    }
    let norm = sum_sq.sqrt();
    if norm > 0.0 {
        for val in &mut emb {
            *val /= norm;
        }
    }
    emb
}

fn serialize_embedding(emb: &[f32]) -> Vec<u8> {
    let mut data_bytes = Vec::with_capacity(emb.len() * 4);
    for val in emb {
        data_bytes.extend_from_slice(&val.to_le_bytes());
    }
    data_bytes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::models::CommandRules;

    #[test]
    fn test_optimize_catalog_embedding() {
        let engine = EmbeddingMatchingEngine::new();
        let catalog = ToolCatalog {
            tool_name: "test_tool".to_string(),
            description: "A filesystem utility to search files".to_string(),
            user_friendly_description: "search".to_string(),
            keywords: "find search grep".to_string(),
            version: "1.0".to_string(),
            options: vec![],
            rules: CommandRules(serde_json::json!({})),
        };

        let result = engine.optimize_catalog(&catalog);
        
        if std::path::Path::new("/home/sandbox-noadmin/RustroverProjects/embedding_models_testing/models/embeddinggemma-300M-BF16.gguf").exists() {
            let optimized = result.unwrap();
            assert_eq!(optimized.tool_name, "test_tool");
            let opt_data = optimized.optimized_data.unwrap();
            assert_eq!(opt_data.key, "gemma_embedding");
            assert_eq!(opt_data.data_type, "BLOB");
            
            assert!(!opt_data.data.is_empty());
            assert_eq!(opt_data.data.len() % 4, 0);

            // Reconstruct float vector and check L2 normalization
            let mut floats = Vec::new();
            for chunk in opt_data.data.chunks_exact(4) {
                let arr: [u8; 4] = chunk.try_into().unwrap();
                floats.push(f32::from_le_bytes(arr));
            }

            let mut sum_sq = 0.0;
            for &f in &floats {
                sum_sq += f * f;
            }
            let norm = sum_sq.sqrt();
            assert!((norm - 1.0).abs() < 1e-4);
        } else {
            assert!(result.is_err());
        }
    }

    #[test]
    fn test_engine_lifecycle_methods() {
        let engine = EmbeddingMatchingEngine::new();
        assert!(engine.load_engines().is_ok());

        let default_engine = EmbeddingMatchingEngine::default();
        assert!(default_engine.load_engines().is_ok());
    }

    #[test]
    fn test_calculate_similarities() {
        let engine = EmbeddingMatchingEngine::new();
        let query = UserQuery {
            query: "list files".to_string(),
            n_grams: None,
        };
        let result = engine.calculate_similarities(&query);
        assert!(result.is_ok());
        let candidates = result.unwrap();
        assert!(!candidates.is_empty());
        assert!(!candidates[0].is_empty());
        assert_eq!(candidates[0][0].option.option_name, "-la");
    }
}
