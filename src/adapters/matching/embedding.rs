use crate::ports::outbound::matching_strategy::MatchingStrategyPort;
use crate::core::errors::AppError;
use crate::core::models::{UserQuery, ScoredCandidate, ScoredTool, CommandOption, ToolCatalog};
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::LlamaModel;

/// Outbound adapter representing the embedding-based matching engine.
#[derive(Clone)]
pub struct EmbeddingMatchingEngine {
    inner: std::sync::Arc<std::sync::Mutex<Option<LlamaModel>>>,
    tool_weight: f64,
    option_weight: f64,
    db_path: String,
    tool_otsu_config: super::otsu::OtsuCutoffConfig,
    option_otsu_config: super::otsu::OtsuCutoffConfig,
}

impl EmbeddingMatchingEngine {
    /// Creates a new EmbeddingMatchingEngine instance.
    pub fn new() -> Self {
        Self {
            inner: std::sync::Arc::new(std::sync::Mutex::new(None)),
            tool_weight: 1.0,
            option_weight: 1.0,
            db_path: "local_assistant.db".to_string(),
            tool_otsu_config: super::otsu::OtsuCutoffConfig::new(0.95, 0.0, 1.0),
            option_otsu_config: super::otsu::OtsuCutoffConfig::new(0.95, 0.0, 1.0),
        }
    }

    /// Sets the tool weight for this engine instance.
    pub fn with_tool_weight(mut self, weight: f64) -> Self {
        self.tool_weight = weight;
        self
    }

    /// Sets the option weight for this engine instance.
    pub fn with_option_weight(mut self, weight: f64) -> Self {
        self.option_weight = weight;
        self
    }

    /// Sets the database path for this engine instance.
    pub fn with_db_path(mut self, db_path: &str) -> Self {
        self.db_path = db_path.to_string();
        self
    }

    /// Sets the Otsu cutoff config for tool retrieval.
    pub fn with_tool_otsu_config(mut self, config: super::otsu::OtsuCutoffConfig) -> Self {
        self.tool_otsu_config = config;
        self
    }

    /// Sets the Otsu cutoff config for option retrieval.
    pub fn with_option_otsu_config(mut self, config: super::otsu::OtsuCutoffConfig) -> Self {
        self.option_otsu_config = config;
        self
    }

    /// Retrieve or initialize the static LLAMA backend safely once per process.
    fn get_global_backend() -> Result<&'static LlamaBackend, AppError> {
        static GLOBAL_BACKEND: std::sync::OnceLock<Result<LlamaBackend, String>> = std::sync::OnceLock::new();
        let res = GLOBAL_BACKEND.get_or_init(|| {
            LlamaBackend::init()
                .map(|mut backend| {
                    backend.void_logs();
                    backend
                })
                .map_err(|e| format!("{:?}", e))
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
            let model_path = "/usr/share/command-finder/models/embeddinggemma-300M-BF16.gguf";

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

fn create_llama_context<'a>(
    backend: &'a LlamaBackend,
    model: &'a LlamaModel,
) -> Result<llama_cpp_2::context::LlamaContext<'a>, AppError> {
    let num_cpus = num_cpus::get_physical();
    let ctx_params = LlamaContextParams::default()
        .with_embeddings(true)
        .with_n_ctx(std::num::NonZeroU32::new(512))
        .with_n_threads(num_cpus as i32)
        .with_n_threads_batch(num_cpus as i32);

    model.new_context(backend, ctx_params)
        .map_err(|e| AppError::Initialization(
            crate::core::errors::InitializationException::new(format!(
                "Failed to create context: {:?}",
                e
            )),
        ))
}

fn ensure_embedding_tables_exist(conn: &rusqlite::Connection, n_embd: u32) -> Result<(), AppError> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS gemma_embedding_optimized_catalogs (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            tool_name TEXT NOT NULL UNIQUE,
            embedding BLOB NOT NULL,
            FOREIGN KEY (tool_name) REFERENCES tool_catalogs(tool_name) ON DELETE CASCADE
        );",
        [],
    ).map_err(|e| AppError::Storage(e.to_string()))?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS gemma_embedding_optimized_options (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            tool_name TEXT NOT NULL,
            option_name TEXT NOT NULL,
            embedding BLOB NOT NULL,
            FOREIGN KEY (tool_name) REFERENCES tool_catalogs(tool_name) ON DELETE CASCADE
        );",
        [],
    ).map_err(|e| AppError::Storage(e.to_string()))?;

    let create_catalog_vec_table = format!(
        "CREATE VIRTUAL TABLE IF NOT EXISTS vec_gemma_embedding_optimized_catalogs USING vec0(
            catalog_id INTEGER PRIMARY KEY,
            gemma_embedding float[{}] distance_metric=cosine
        );",
        n_embd
    );
    conn.execute(&create_catalog_vec_table, []).map_err(|e| AppError::Storage(e.to_string()))?;

    let create_vec_table = format!(
        "CREATE VIRTUAL TABLE IF NOT EXISTS vec_gemma_embedding_optimized_options USING vec0(
            option_id INTEGER PRIMARY KEY,
            gemma_embedding float[{}] distance_metric=cosine
        );",
        n_embd
    );
    conn.execute(&create_vec_table, []).map_err(|e| AppError::Storage(e.to_string()))?;

    Ok(())
}

fn clear_existing_embedding_records(conn: &rusqlite::Connection, tool_name: &str) -> Result<(), AppError> {
    conn.execute(
        "DELETE FROM vec_gemma_embedding_optimized_catalogs WHERE catalog_id IN (
            SELECT id FROM gemma_embedding_optimized_catalogs WHERE tool_name = ?
        )",
        rusqlite::params![tool_name],
    ).map_err(|e| AppError::Storage(e.to_string()))?;

    conn.execute(
        "DELETE FROM vec_gemma_embedding_optimized_options WHERE option_id IN (
            SELECT id FROM gemma_embedding_optimized_options WHERE tool_name = ?
        )",
        rusqlite::params![tool_name],
    ).map_err(|e| AppError::Storage(e.to_string()))?;

    conn.execute(
        "DELETE FROM gemma_embedding_optimized_catalogs WHERE tool_name = ?",
        rusqlite::params![tool_name],
    ).map_err(|e| AppError::Storage(e.to_string()))?;

    conn.execute(
        "DELETE FROM gemma_embedding_optimized_options WHERE tool_name = ?",
        rusqlite::params![tool_name],
    ).map_err(|e| AppError::Storage(e.to_string()))?;

    Ok(())
}

fn compute_and_save_tool_embedding(
    conn: &rusqlite::Connection,
    model: &LlamaModel,
    ctx: &mut llama_cpp_2::context::LlamaContext,
    catalog: &ToolCatalog,
) -> Result<(), AppError> {
    let processed_text = format!(
        "title: {} | text: {} Keywords: {}",
        catalog.tool_name, catalog.description, catalog.keywords
    );
    let raw_emb = compute_embedding(model, ctx, &processed_text)?;
    let normalized_emb = l2_normalize(raw_emb);
    let data_bytes = serialize_embedding(&normalized_emb);

    conn.execute(
        "INSERT INTO gemma_embedding_optimized_catalogs (tool_name, embedding) VALUES (?, ?)",
        rusqlite::params![catalog.tool_name, data_bytes],
    ).map_err(|e| AppError::Storage(e.to_string()))?;

    let catalog_id = conn.last_insert_rowid();

    conn.execute(
        "INSERT INTO vec_gemma_embedding_optimized_catalogs (catalog_id, gemma_embedding) VALUES (?, ?)",
        rusqlite::params![catalog_id, data_bytes],
    ).map_err(|e| AppError::Storage(e.to_string()))?;

    Ok(())
}

fn compute_and_save_options_embeddings(
    conn: &rusqlite::Connection,
    model: &LlamaModel,
    ctx: &mut llama_cpp_2::context::LlamaContext,
    catalog: &ToolCatalog,
) -> Result<(), AppError> {
    let mut opt_stmt = conn.prepare(
        "INSERT INTO gemma_embedding_optimized_options (tool_name, option_name, embedding) VALUES (?, ?, ?)"
    ).map_err(|e| AppError::Storage(e.to_string()))?;

    for opt in &catalog.options {
        let processed_opt_text = format!(
            "title: {} {} | text: {} Keywords: {}",
            catalog.tool_name, opt.option_name, opt.description, opt.keywords
        );
        let opt_raw_emb = compute_embedding(model, ctx, &processed_opt_text)?;
        let opt_normalized_emb = l2_normalize(opt_raw_emb);
        let opt_data_bytes = serialize_embedding(&opt_normalized_emb);

        opt_stmt.execute(rusqlite::params![
            catalog.tool_name,
            opt.option_name,
            opt_data_bytes,
        ]).map_err(|e| AppError::Storage(e.to_string()))?;

        let option_id = conn.last_insert_rowid();

        conn.execute(
            "INSERT INTO vec_gemma_embedding_optimized_options (option_id, gemma_embedding) VALUES (?, ?)",
            rusqlite::params![option_id, opt_data_bytes],
        ).map_err(|e| AppError::Storage(e.to_string()))?;
    }

    Ok(())
}

fn fetch_matching_tools(
    conn: &rusqlite::Connection,
    query_emb: &[u8],
) -> Result<Vec<ScoredTool>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT 
            c.tool_name, 
            c.description, 
            c.user_friendly_description, 
            c.keywords, 
            c.version, 
            c.rules, 
            v.distance
         FROM vec_gemma_embedding_optimized_catalogs v
         JOIN gemma_embedding_optimized_catalogs ec ON v.catalog_id = ec.id
         JOIN tool_catalogs c ON ec.tool_name = c.tool_name
         WHERE v.gemma_embedding MATCH ?1 AND k = 10
         ORDER BY v.distance ASC"
    ).map_err(|e| AppError::Storage(e.to_string()))?;

    let mut rows = stmt.query(rusqlite::params![query_emb])
        .map_err(|e| AppError::Storage(e.to_string()))?;

    let mut tools = Vec::new();
    while let Some(row) = rows.next().map_err(|e| AppError::Storage(e.to_string()))? {
        let tool_name: String = row.get(0).map_err(|e| AppError::Storage(e.to_string()))?;
        let description: String = row.get(1).map_err(|e| AppError::Storage(e.to_string()))?;
        let user_friendly_description: String = row.get(2).map_err(|e| AppError::Storage(e.to_string()))?;
        let keywords: String = row.get(3).map_err(|e| AppError::Storage(e.to_string()))?;
        let version: String = row.get(4).map_err(|e| AppError::Storage(e.to_string()))?;
        let rules_str: String = row.get(5).map_err(|e| AppError::Storage(e.to_string()))?;
        let distance: f64 = row.get(6).map_err(|e| AppError::Storage(e.to_string()))?;

        let rules_val: serde_json::Value = serde_json::from_str(&rules_str).unwrap_or(serde_json::Value::Null);
        let score = 1.0 - distance;

        tools.push(ScoredTool {
            tool: ToolCatalog {
                tool_name,
                description,
                user_friendly_description,
                keywords,
                version,
                options: vec![],
                rules: crate::core::models::CommandRules(rules_val),
            },
            score,
        });
    }
    Ok(tools)
}

fn fetch_matching_options(
    conn: &rusqlite::Connection,
    query_emb: &[u8],
    tool_name: &str,
) -> Result<Vec<ScoredCandidate>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT 
            o.option_name, 
            o.description, 
            o.user_friendly_description, 
            o.keywords, 
            v.distance
         FROM vec_gemma_embedding_optimized_options v
         JOIN gemma_embedding_optimized_options eo ON v.option_id = eo.id
         JOIN command_options o ON eo.tool_name = o.tool_name AND eo.option_name = o.option_name
         WHERE v.gemma_embedding MATCH ?1 AND eo.tool_name = ?2 AND k = 100
         ORDER BY v.distance ASC"
    ).map_err(|e| AppError::Storage(e.to_string()))?;

    let mut rows = stmt.query(rusqlite::params![query_emb, tool_name])
        .map_err(|e| AppError::Storage(e.to_string()))?;

    let mut candidates = Vec::new();
    while let Some(row) = rows.next().map_err(|e| AppError::Storage(e.to_string()))? {
        let option_name: String = row.get(0).map_err(|e| AppError::Storage(e.to_string()))?;
        let description: String = row.get(1).map_err(|e| AppError::Storage(e.to_string()))?;
        let user_friendly_description: String = row.get(2).map_err(|e| AppError::Storage(e.to_string()))?;
        let keywords: String = row.get(3).map_err(|e| AppError::Storage(e.to_string()))?;
        let distance: f64 = row.get(4).map_err(|e| AppError::Storage(e.to_string()))?;

        let score = 1.0 - distance;

        candidates.push(ScoredCandidate {
            option: CommandOption {
                option_name,
                description,
                user_friendly_description,
                keywords,
            },
            score,
        });
    }
    Ok(candidates)
}

impl MatchingStrategyPort for EmbeddingMatchingEngine {
    fn find_tools(
        &self,
        query: &UserQuery,
    ) -> Result<Vec<ScoredTool>, AppError> {
        crate::adapters::persistence::register_sqlite_vec();

        let backend = Self::get_global_backend()?;
        let guard = self.get_or_init_model()?;
        let model = guard.as_ref().unwrap();

        let mut ctx = create_llama_context(backend, model)?;

        let query_raw_emb = compute_embedding(model, &mut ctx, &query.query)?;
        let query_normalized_emb = l2_normalize(query_raw_emb);
        let query_data_bytes = serialize_embedding(&query_normalized_emb);

        let conn = rusqlite::Connection::open(&self.db_path)
            .map_err(|e| AppError::Storage(format!("Failed to open DB: {}", e)))?;
        let _ = conn.execute("PRAGMA busy_timeout = 5000;", []);

        let mut results = fetch_matching_tools(&conn, &query_data_bytes)?;
        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        let filtered = super::otsu::apply_otsu_cutoff(results, &self.tool_otsu_config);
        Ok(filtered)
    }

    fn find_options(
        &self,
        query: &UserQuery,
        tool_name: &str,
    ) -> Result<Vec<ScoredCandidate>, AppError> {
        crate::adapters::persistence::register_sqlite_vec();

        let backend = Self::get_global_backend()?;
        let guard = self.get_or_init_model()?;
        let model = guard.as_ref().unwrap();

        let mut ctx = create_llama_context(backend, model)?;

        let query_raw_emb = compute_embedding(model, &mut ctx, &query.query)?;
        let query_normalized_emb = l2_normalize(query_raw_emb);
        let query_data_bytes = serialize_embedding(&query_normalized_emb);

        let conn = rusqlite::Connection::open(&self.db_path)
            .map_err(|e| AppError::Storage(format!("Failed to open DB: {}", e)))?;
        let _ = conn.execute("PRAGMA busy_timeout = 5000;", []);

        let mut results = fetch_matching_options(&conn, &query_data_bytes, tool_name)?;
        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        let filtered = super::otsu::apply_otsu_cutoff(results, &self.option_otsu_config);
        Ok(filtered)
    }

    fn load_engines(&self) -> Result<bool, AppError> {
        let _guard = self.get_or_init_model()?;
        Ok(true)
    }

    fn create_optimized_catalog(
        &self,
        catalog: &ToolCatalog,
    ) -> Result<(), AppError> {
        crate::adapters::persistence::register_sqlite_vec();
        let conn = rusqlite::Connection::open(&self.db_path)
            .map_err(|e| AppError::Storage(format!("Failed to open DB: {}", e)))?;
        let _ = conn.execute("PRAGMA busy_timeout = 5000;", []);

        let backend = Self::get_global_backend()?;
        let guard = self.get_or_init_model()?;
        let model = guard.as_ref().unwrap();
        let n_embd = model.n_embd() as u32;

        ensure_embedding_tables_exist(&conn, n_embd)?;
        clear_existing_embedding_records(&conn, &catalog.tool_name)?;

        let mut ctx = create_llama_context(backend, model)?;

        compute_and_save_tool_embedding(&conn, model, &mut ctx, catalog)?;
        compute_and_save_options_embeddings(&conn, model, &mut ctx, catalog)?;

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
        crate::adapters::persistence::register_sqlite_vec();
        let conn = rusqlite::Connection::open(&self.db_path)
            .map_err(|e| AppError::Storage(format!("Failed to open DB: {}", e)))?;
        let _ = conn.execute("PRAGMA busy_timeout = 5000;", []);

        conn.execute(
            "DELETE FROM vec_gemma_embedding_optimized_catalogs WHERE catalog_id IN (
                SELECT id FROM gemma_embedding_optimized_catalogs WHERE tool_name = ?
            )",
            rusqlite::params![tool_name],
        ).map_err(|e| AppError::Storage(e.to_string()))?;

        conn.execute(
            "DELETE FROM vec_gemma_embedding_optimized_options WHERE option_id IN (
                SELECT id FROM gemma_embedding_optimized_options WHERE tool_name = ?
            )",
            rusqlite::params![tool_name],
        ).map_err(|e| AppError::Storage(e.to_string()))?;

        conn.execute(
            "DELETE FROM gemma_embedding_optimized_catalogs WHERE tool_name = ?",
            rusqlite::params![tool_name],
        ).map_err(|e| AppError::Storage(e.to_string()))?;

        conn.execute(
            "DELETE FROM gemma_embedding_optimized_options WHERE tool_name = ?",
            rusqlite::params![tool_name],
        ).map_err(|e| AppError::Storage(e.to_string()))?;

        Ok(())
    }

    fn tool_weight(&self) -> f64 {
        self.tool_weight
    }

    fn option_weight(&self) -> f64 {
        self.option_weight
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

    fn cleanup_db(db_path: &str) {
        let _ = std::fs::remove_file(db_path);
        let _ = std::fs::remove_file(format!("{}-shm", db_path));
        let _ = std::fs::remove_file(format!("{}-wal", db_path));
    }

    #[test]
    fn test_optimize_catalog_embedding() {
        use crate::ports::outbound::storage::StoragePort;
        use crate::adapters::persistence::PersistenceAdapter;

        let test_db = format!("test_assistant_emb_opt_{}.db", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos());
        let storage = PersistenceAdapter::new().with_db_path(&test_db);
        let engine = EmbeddingMatchingEngine::new().with_db_path(&test_db);
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
            let conn = rusqlite::Connection::open(&test_db).unwrap();
            let mut stmt1 = conn.prepare("SELECT embedding FROM gemma_embedding_optimized_catalogs WHERE tool_name = ?").unwrap();
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
            let mut stmt2 = conn.prepare("SELECT option_name, embedding FROM gemma_embedding_optimized_options WHERE tool_name = ?").unwrap();
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
        cleanup_db(&test_db);
    }

    #[test]
    fn test_engine_lifecycle_methods() {
        let engine = EmbeddingMatchingEngine::new();
        assert!(engine.load_engines().is_ok());

        let default_engine = EmbeddingMatchingEngine::default();
        assert!(default_engine.load_engines().is_ok());
    }

    #[test]
    fn test_find_tools_and_options() {
        use crate::ports::outbound::storage::StoragePort;
        use crate::adapters::persistence::PersistenceAdapter;

        let test_db = format!("test_assistant_emb_find_{}.db", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos());
        let storage = PersistenceAdapter::new().with_db_path(&test_db);
        let engine = EmbeddingMatchingEngine::new().with_db_path(&test_db);
        
        let tool_name = "test_similarity_tool";
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

        if std::path::Path::new("/home/sandbox-noadmin/RustroverProjects/embedding_models_testing/models/embeddinggemma-300M-BF16.gguf").exists() {
            storage.save_catalog(&catalog).unwrap();
            engine.create_optimized_catalog(&catalog).unwrap();

            let tool_query = UserQuery {
                query: "A filesystem utility to search files".to_string(),
                n_grams: None,
            };
            let tools_res = engine.find_tools(&tool_query);
            assert!(tools_res.is_ok(), "Expected Ok for find_tools, got error: {:?}", tools_res.err());
            let tools = tools_res.unwrap();
            assert!(!tools.is_empty(), "Tools list should not be empty");
            assert!(tools.iter().any(|t| t.tool.tool_name == tool_name));

            let opt_query = UserQuery {
                query: "Search subdirectories recursively".to_string(),
                n_grams: None,
            };
            let options_res = engine.find_options(&opt_query, tool_name);
            assert!(options_res.is_ok(), "Expected Ok for find_options, got error: {:?}", options_res.err());
            let options = options_res.unwrap();
            assert!(!options.is_empty(), "Options list should not be empty");
            assert_eq!(options[0].option.option_name, "--recursive");

            // Clean up
            let _ = storage.delete_catalog(tool_name);
            let _ = engine.delete_optimized_catalog(tool_name);
        } else {
            let query = UserQuery {
                query: "Search subdirectories recursively".to_string(),
                n_grams: None,
            };
            let result = engine.find_tools(&query);
            assert!(result.is_err());
            let result_opt = engine.find_options(&query, tool_name);
            assert!(result_opt.is_err());
        }
        cleanup_db(&test_db);
    }
}
