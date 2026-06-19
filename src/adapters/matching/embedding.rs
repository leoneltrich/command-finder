use crate::ports::outbound::matching_strategy::MatchingStrategyPort;
use crate::core::errors::AppError;
use crate::core::models::{UserQuery, ScoredCandidate, CommandOption, ToolCatalog};
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::LlamaModel;

/// Outbound adapter representing the embedding-based matching engine.
#[derive(Clone)]
pub struct EmbeddingMatchingEngine {
    inner: std::sync::Arc<std::sync::Mutex<Option<LlamaModel>>>,
}

impl EmbeddingMatchingEngine {
    /// Creates a new EmbeddingMatchingEngine instance.
    pub fn new() -> Self {
        Self {
            inner: std::sync::Arc::new(std::sync::Mutex::new(None)),
        }
    }

    /// Retrieve or initialize the static LLAMA backend safely once per process.
    fn get_global_backend() -> Result<&'static LlamaBackend, AppError> {
        static GLOBAL_BACKEND: std::sync::OnceLock<Result<LlamaBackend, String>> = std::sync::OnceLock::new();
        let res = GLOBAL_BACKEND.get_or_init(|| {
            LlamaBackend::init().map_err(|e| format!("{:?}", e))
        });
        res.as_ref().map_err(|err| AppError::Initialization(
            crate::core::errors::InitializationException::new(format!(
                "Failed to initialize llama-cpp backend: {}",
                err
            )),
        ))
    }

    /// Centralized fallible lazy model loader
    fn get_or_init_model(&self) -> Result<std::sync::MutexGuard<'_, Option<LlamaModel>>, AppError> {
        let mut guard = self.inner.lock().map_err(|e| AppError::Initialization(
            crate::core::errors::InitializationException::new(format!(
                "Mutex poisoned: {:?}",
                e
            )),
        ))?;

        if guard.is_none() {
            let model_path = "/home/sandbox-noadmin/RustroverProjects/embedding_models_testing/models/embeddinggemma-300M-BF16.gguf";

            if !std::path::Path::new(model_path).exists() {
                return Err(AppError::Initialization(
                    crate::core::errors::InitializationException::new(format!(
                        "Model file does not exist at {}",
                        model_path
                    )),
                ));
            }

            let backend = Self::get_global_backend()?;
            let model_params = LlamaModelParams::default();
            let model = LlamaModel::load_from_file(backend, model_path, &model_params)
                .map_err(|e| AppError::Initialization(
                    crate::core::errors::InitializationException::new(format!(
                        "Failed to load model: {:?}",
                        e
                    )),
                ))?;

            *guard = Some(model);
        }

        Ok(guard)
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
        let _guard = self.get_or_init_model()?;
        Ok(true)
    }

    fn create_optimized_catalog(
        &self,
        catalog: &ToolCatalog,
    ) -> Result<(), AppError> {
        let conn = rusqlite::Connection::open("local_assistant.db")
            .map_err(|e| AppError::Storage(format!("Failed to open DB: {}", e)))?;

        // 1. Create tables if they do not exist
        conn.execute(
            "CREATE TABLE IF NOT EXISTS embedding_optimized_catalogs (
                tool_name TEXT PRIMARY KEY,
                embedding BLOB NOT NULL,
                FOREIGN KEY (tool_name) REFERENCES tool_catalogs(tool_name) ON DELETE CASCADE
            );",
            [],
        ).map_err(|e| AppError::Storage(e.to_string()))?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS embedding_optimized_options (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                tool_name TEXT NOT NULL,
                option_name TEXT NOT NULL,
                embedding BLOB NOT NULL,
                FOREIGN KEY (tool_name) REFERENCES tool_catalogs(tool_name) ON DELETE CASCADE
            );",
            [],
        ).map_err(|e| AppError::Storage(e.to_string()))?;

        // 2. Clear any existing records for this tool to prevent duplicates (idempotency)
        conn.execute(
            "DELETE FROM embedding_optimized_catalogs WHERE tool_name = ?",
            rusqlite::params![catalog.tool_name],
        ).map_err(|e| AppError::Storage(e.to_string()))?;

        conn.execute(
            "DELETE FROM embedding_optimized_options WHERE tool_name = ?",
            rusqlite::params![catalog.tool_name],
        ).map_err(|e| AppError::Storage(e.to_string()))?;

        // 3. Compute parent tool embedding
        let num_cpus = num_cpus::get_physical();
        let backend = Self::get_global_backend()?;
        let guard = self.get_or_init_model()?;
        let model = guard.as_ref().unwrap();

        let ctx_params = LlamaContextParams::default()
            .with_embeddings(true)
            .with_n_ctx(std::num::NonZeroU32::new(512))
            .with_n_threads(num_cpus as i32)
            .with_n_threads_batch(num_cpus as i32);

        let mut ctx = model.new_context(backend, ctx_params)
            .map_err(|e| AppError::Initialization(
                crate::core::errors::InitializationException::new(format!(
                    "Failed to create context: {:?}",
                    e
                )),
            ))?;

        let processed_text = format!("title: {} | text: {}", catalog.tool_name, catalog.description);
        let raw_emb = compute_embedding(model, &mut ctx, &processed_text)?;
        let normalized_emb = l2_normalize(raw_emb);
        let data_bytes = serialize_embedding(&normalized_emb);

        conn.execute(
            "INSERT INTO embedding_optimized_catalogs (tool_name, embedding) VALUES (?, ?)",
            rusqlite::params![catalog.tool_name, data_bytes],
        ).map_err(|e| AppError::Storage(e.to_string()))?;

        // 4. Compute and save options embeddings
        let mut opt_stmt = conn.prepare(
            "INSERT INTO embedding_optimized_options (tool_name, option_name, embedding) VALUES (?, ?, ?)"
        ).map_err(|e| AppError::Storage(e.to_string()))?;

        for opt in &catalog.options {
            let processed_opt_text = format!(
                "title: {} {} | text: {} Keywords: {}",
                catalog.tool_name, opt.option_name, opt.description, opt.keywords
            );
            let opt_raw_emb = compute_embedding(model, &mut ctx, &processed_opt_text)?;
            let opt_normalized_emb = l2_normalize(opt_raw_emb);
            let opt_data_bytes = serialize_embedding(&opt_normalized_emb);

            opt_stmt.execute(rusqlite::params![
                catalog.tool_name,
                opt.option_name,
                opt_data_bytes,
            ]).map_err(|e| AppError::Storage(e.to_string()))?;
        }

        Ok(())
    }

    fn update_optimized_catalog(
        &self,
        catalog: &ToolCatalog,
    ) -> Result<(), AppError> {
        self.create_optimized_catalog(catalog)
    }

    fn delete_optimized_catalog(
        &self,
        tool_name: &str,
    ) -> Result<(), AppError> {
        let conn = rusqlite::Connection::open("local_assistant.db")
            .map_err(|e| AppError::Storage(format!("Failed to open DB: {}", e)))?;

        conn.execute(
            "DELETE FROM embedding_optimized_catalogs WHERE tool_name = ?",
            rusqlite::params![tool_name],
        ).map_err(|e| AppError::Storage(e.to_string()))?;

        conn.execute(
            "DELETE FROM embedding_optimized_options WHERE tool_name = ?",
            rusqlite::params![tool_name],
        ).map_err(|e| AppError::Storage(e.to_string()))?;

        Ok(())
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
        use crate::ports::outbound::storage::StoragePort;
        use crate::adapters::persistence::PersistenceAdapter;

        let storage = PersistenceAdapter::new();
        let engine = EmbeddingMatchingEngine::new();
        let tool_name = "test_tool";
        let _ = storage.delete_catalog(tool_name);
        let _ = engine.delete_optimized_catalog(tool_name);

        let catalog = ToolCatalog {
            tool_name: tool_name.to_string(),
            description: "A filesystem utility to search files".to_string(),
            user_friendly_description: "search".to_string(),
            keywords: "find search grep".to_string(),
            version: "1.0".to_string(),
            options: vec![
                CommandOption {
                    option_name: "--recursive".to_string(),
                    description: "Search subdirectories recursively".to_string(),
                    user_friendly_description: "recursive search".to_string(),
                    keywords: "recursive subdirectories all depth".to_string(),
                }
            ],
            rules: CommandRules(serde_json::json!({})),
        };

        // 1. Ingest base catalog first (required due to foreign keys)
        storage.save_catalog(&catalog).unwrap();

        let result = engine.create_optimized_catalog(&catalog);
        
        if std::path::Path::new("/home/sandbox-noadmin/RustroverProjects/embedding_models_testing/models/embeddinggemma-300M-BF16.gguf").exists() {
            assert!(result.is_ok(), "Expected Ok, got: {:?}", result);

            // Query database directly to check parent and options embeddings
            let conn = rusqlite::Connection::open("local_assistant.db").unwrap();
            let mut stmt1 = conn.prepare("SELECT embedding FROM embedding_optimized_catalogs WHERE tool_name = ?").unwrap();
            let mut row1 = stmt1.query(rusqlite::params![tool_name]).unwrap();
            let r1 = row1.next().unwrap().unwrap();
            let data: Vec<u8> = r1.get(0).unwrap();
            
            assert!(!data.is_empty());
            assert_eq!(data.len() % 4, 0);

            // Reconstruct float vector and check L2 normalization
            let mut floats = Vec::new();
            for chunk in data.chunks_exact(4) {
                let arr: [u8; 4] = chunk.try_into().unwrap();
                floats.push(f32::from_le_bytes(arr));
            }

            let mut sum_sq = 0.0;
            for &f in &floats {
                sum_sq += f * f;
            }
            let norm = sum_sq.sqrt();
            assert!((norm - 1.0).abs() < 1e-4);

            // Assert option embedding was generated & L2 normalized
            let mut stmt2 = conn.prepare("SELECT option_name, embedding FROM embedding_optimized_options WHERE tool_name = ?").unwrap();
            let mut row2 = stmt2.query(rusqlite::params![tool_name]).unwrap();
            let r2 = row2.next().unwrap().unwrap();
            let opt_name: String = r2.get(0).unwrap();
            let opt_data: Vec<u8> = r2.get(1).unwrap();
            assert_eq!(opt_name, "--recursive");
            assert!(!opt_data.is_empty());
            assert_eq!(opt_data.len() % 4, 0);

            let mut opt_floats = Vec::new();
            for chunk in opt_data.chunks_exact(4) {
                let arr: [u8; 4] = chunk.try_into().unwrap();
                opt_floats.push(f32::from_le_bytes(arr));
            }
            let mut opt_sum_sq = 0.0;
            for &f in &opt_floats {
                opt_sum_sq += f * f;
            }
            let opt_norm = opt_sum_sq.sqrt();
            assert!((opt_norm - 1.0).abs() < 1e-4);
        } else {
            assert!(result.is_err());
        }

        // Clean up
        let _ = storage.delete_catalog(tool_name);
        let _ = engine.delete_optimized_catalog(tool_name);
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
