use crate::ports::outbound::matching_strategy::MatchingStrategyPort;
use crate::core::errors::AppError;
use crate::core::models::{UserQuery, ScoredCandidate, CommandOption, ToolCatalog};
use std::collections::HashSet;
use rust_stemmers::{Algorithm, Stemmer};
use tantivy::schema::{Schema, Field, STORED, TEXT, STRING, Value};
use tantivy::{Index, IndexReader, doc};
use tantivy::query::QueryParser;
use tantivy::collector::TopDocs;

pub struct TantivyIndexState {
    pub reader: IndexReader,
    pub query_parser: QueryParser,
    pub schema: Schema,
    pub description_field: Field,
    pub keywords_field: Field,
    pub option_name_field: Field,
    pub tool_name_field: Field,
    pub raw_description_field: Field,
    pub user_friendly_description_field: Field,
    pub raw_keywords_field: Field,
    pub catalog_keywords: HashSet<String>,
}

/// Outbound adapter representing the keyword-based matching engine.
#[derive(Clone)]
pub struct KeywordMatchingEngine {
    index_state: std::sync::Arc<std::sync::RwLock<Option<TantivyIndexState>>>,
}

impl KeywordMatchingEngine {
    /// Creates a new KeywordMatchingEngine instance.
    pub fn new() -> Self {
        Self {
            index_state: std::sync::Arc::new(std::sync::RwLock::new(None)),
        }
    }
}

impl Default for KeywordMatchingEngine {
    fn default() -> Self {
        Self::new()
    }
}

fn clean_and_tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .replace('\'', "")
        .replace('’', "")
        .replace('/', " ")
        .replace(':', " ")
        .split_whitespace()
        .map(|word| {
            word.chars()
                .filter(|c| c.is_alphanumeric() || *c == '-')
                .collect::<String>()
        })
        .filter(|w| !w.is_empty())
        .collect()
}

fn filter_stop_words(
    tokens: Vec<String>,
    stop_words: &HashSet<String>,
    bypass_words: &HashSet<String>,
) -> Vec<String> {
    tokens
        .into_iter()
        .filter(|w| !stop_words.contains(w) || bypass_words.contains(w))
        .collect()
}

fn stem_tokens(tokens: Vec<String>, stemmer: &Stemmer) -> Vec<String> {
    tokens
        .into_iter()
        .map(|w| stemmer.stem(&w).into_owned())
        .collect()
}

fn generate_shingles(tokens: &[String]) -> Vec<String> {
    let mut shingles = Vec::new();
    for window in tokens.windows(2) {
        shingles.push(format!("{}{}", window[0], window[1]));
    }
    for window in tokens.windows(3) {
        shingles.push(format!("{}{}{}", window[0], window[1], window[2]));
    }
    shingles
}

fn build_bm25_optimized_data(description: &str, keywords: &str) -> (String, String) {
    let stemmer = Stemmer::create(Algorithm::English);

    let raw_keywords = clean_and_tokenize(keywords);
    let keyword_set: HashSet<String> = raw_keywords.iter().cloned().collect();
    let stemmed_keywords = stem_tokens(raw_keywords, &stemmer);

    let english_stops: HashSet<String> = stop_words::get(stop_words::LANGUAGE::English)
        .iter()
        .map(|s| s.to_string())
        .collect();

    let raw_desc_tokens = clean_and_tokenize(description);
    let filtered_desc_tokens = filter_stop_words(raw_desc_tokens.clone(), &english_stops, &keyword_set);
    let stemmed_desc_tokens = stem_tokens(filtered_desc_tokens, &stemmer);
    let preprocessed_desc = stemmed_desc_tokens.join(" ");

    let stemmed_raw_desc_tokens = stem_tokens(raw_desc_tokens, &stemmer);
    let shingles = generate_shingles(&stemmed_raw_desc_tokens);

    let mut final_keywords = stemmed_keywords;
    final_keywords.extend(shingles);
    let preprocessed_keywords = final_keywords.join(" ");

    (preprocessed_desc, preprocessed_keywords)
}

fn preprocess_query_string(query_text: &str, catalog_keywords: &HashSet<String>) -> String {
    let stemmer = Stemmer::create(Algorithm::English);
    let query_tokens = clean_and_tokenize(query_text);
    
    // Generate query shingles from raw, unstemmed tokens
    let shingles = generate_shingles(&query_tokens);

    let english_stops: HashSet<String> = stop_words::get(stop_words::LANGUAGE::English)
        .iter()
        .map(|s| s.to_string())
        .collect();
    let filtered_tokens = filter_stop_words(query_tokens, &english_stops, catalog_keywords);
    let stemmed_tokens = stem_tokens(filtered_tokens, &stemmer);

    let mut final_query_tokens = stemmed_tokens;
    final_query_tokens.extend(shingles);

    // Escape hyphens
    let escaped_tokens: Vec<String> = final_query_tokens
        .into_iter()
        .map(|t| t.replace('-', "\\-"))
        .collect();
    
    escaped_tokens.join(" ")
}

struct TantivyFields {
    description_field: Field,
    keywords_field: Field,
    option_name_field: Field,
    tool_name_field: Field,
    raw_description_field: Field,
    user_friendly_description_field: Field,
    raw_keywords_field: Field,
}

fn map_search_results_to_candidates(
    top_docs: Vec<(f32, tantivy::DocAddress)>,
    searcher: &tantivy::Searcher,
    state: &TantivyIndexState,
) -> Result<Vec<ScoredCandidate>, AppError> {
    let mut candidates = Vec::new();
    for (score, doc_address) in top_docs {
        let doc: tantivy::TantivyDocument = searcher.doc(doc_address)
            .map_err(|e| AppError::Matching(format!("Failed to retrieve doc: {}", e)))?;

        let option_name = doc.get_first(state.option_name_field)
            .and_then(|v| v.as_leaf().and_then(|l| l.as_str()))
            .unwrap_or("")
            .to_string();
        let description = doc.get_first(state.raw_description_field)
            .and_then(|v| v.as_leaf().and_then(|l| l.as_str()))
            .unwrap_or("")
            .to_string();
        let user_friendly_description = doc.get_first(state.user_friendly_description_field)
            .and_then(|v| v.as_leaf().and_then(|l| l.as_str()))
            .unwrap_or("")
            .to_string();
        let keywords = doc.get_first(state.raw_keywords_field)
            .and_then(|v| v.as_leaf().and_then(|l| l.as_str()))
            .unwrap_or("")
            .to_string();

        candidates.push(ScoredCandidate {
            option: CommandOption {
                option_name,
                description,
                user_friendly_description,
                keywords,
            },
            score: score as f64,
        });
    }
    Ok(candidates)
}

fn fetch_catalog_keywords(conn: &rusqlite::Connection) -> Result<HashSet<String>, AppError> {
    let mut keyword_set = HashSet::new();

    let mut stmt1 = conn.prepare("SELECT keywords FROM tool_catalogs")
        .map_err(|e| AppError::Storage(e.to_string()))?;
    let mut rows1 = stmt1.query([]).map_err(|e| AppError::Storage(e.to_string()))?;
    while let Some(row) = rows1.next().map_err(|e| AppError::Storage(e.to_string()))? {
        let kw: String = row.get(0).map_err(|e| AppError::Storage(e.to_string()))?;
        for word in clean_and_tokenize(&kw) {
            keyword_set.insert(word);
        }
    }

    let mut stmt2 = conn.prepare("SELECT keywords FROM command_options")
        .map_err(|e| AppError::Storage(e.to_string()))?;
    let mut rows2 = stmt2.query([]).map_err(|e| AppError::Storage(e.to_string()))?;
    while let Some(row) = rows2.next().map_err(|e| AppError::Storage(e.to_string()))? {
        let kw: String = row.get(0).map_err(|e| AppError::Storage(e.to_string()))?;
        for word in clean_and_tokenize(&kw) {
            keyword_set.insert(word);
        }
    }

    Ok(keyword_set)
}

fn check_optimized_tables_exist(conn: &rusqlite::Connection) -> bool {
    if let Ok(mut stmt) = conn.prepare("SELECT name FROM sqlite_master WHERE type='table' AND (name='bm25_optimized_catalogs' OR name='bm25_optimized_options')") {
        if let Ok(mut rows) = stmt.query([]) {
            let mut count = 0;
            while let Some(_) = rows.next().unwrap_or(None) {
                count += 1;
            }
            return count == 2;
        }
    }
    false
}

fn ingest_documents(
    conn: &rusqlite::Connection,
    index_writer: &mut tantivy::IndexWriter,
    state_fields: &TantivyFields,
) -> Result<(), AppError> {
    if !check_optimized_tables_exist(conn) {
        return Ok(());
    }

    let mut stmt = conn.prepare(
        "SELECT 
            c.tool_name, 
            c.option_name, 
            c.description, 
            c.user_friendly_description, 
            c.keywords, 
            o.preprocessed_description, 
            o.preprocessed_keywords 
         FROM command_options c
         INNER JOIN bm25_optimized_options o 
             ON c.tool_name = o.tool_name AND c.option_name = o.option_name"
    ).map_err(|e| AppError::Storage(e.to_string()))?;
    let mut rows = stmt.query([]).map_err(|e| AppError::Storage(e.to_string()))?;
    while let Some(row) = rows.next().map_err(|e| AppError::Storage(e.to_string()))? {
        let tool_name: String = row.get(0).map_err(|e| AppError::Storage(e.to_string()))?;
        let option_name: String = row.get(1).map_err(|e| AppError::Storage(e.to_string()))?;
        let raw_desc: String = row.get(2).map_err(|e| AppError::Storage(e.to_string()))?;
        let raw_user_desc: String = row.get(3).map_err(|e| AppError::Storage(e.to_string()))?;
        let raw_kws: String = row.get(4).map_err(|e| AppError::Storage(e.to_string()))?;
        let preprocessed_desc: String = row.get(5).map_err(|e| AppError::Storage(e.to_string()))?;
        let preprocessed_kws: String = row.get(6).map_err(|e| AppError::Storage(e.to_string()))?;

        let doc = doc!(
            state_fields.tool_name_field => tool_name,
            state_fields.option_name_field => option_name,
            state_fields.raw_description_field => raw_desc,
            state_fields.user_friendly_description_field => raw_user_desc,
            state_fields.raw_keywords_field => raw_kws,
            state_fields.description_field => preprocessed_desc,
            state_fields.keywords_field => preprocessed_kws
        );
        index_writer.add_document(doc).map_err(|e| AppError::Matching(e.to_string()))?;
    }
    Ok(())
}

fn ensure_bm25_tables_exist(conn: &rusqlite::Connection) -> Result<(), AppError> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS bm25_optimized_catalogs (
            tool_name TEXT PRIMARY KEY,
            preprocessed_description TEXT NOT NULL,
            preprocessed_keywords TEXT NOT NULL,
            FOREIGN KEY (tool_name) REFERENCES tool_catalogs(tool_name) ON DELETE CASCADE
        );",
        [],
    ).map_err(|e| AppError::Storage(e.to_string()))?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS bm25_optimized_options (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            tool_name TEXT NOT NULL,
            option_name TEXT NOT NULL,
            preprocessed_description TEXT NOT NULL,
            preprocessed_keywords TEXT NOT NULL,
            FOREIGN KEY (tool_name) REFERENCES tool_catalogs(tool_name) ON DELETE CASCADE
        );",
        [],
    ).map_err(|e| AppError::Storage(e.to_string()))?;

    Ok(())
}

fn clear_existing_bm25_records(conn: &rusqlite::Connection, tool_name: &str) -> Result<(), AppError> {
    conn.execute(
        "DELETE FROM bm25_optimized_catalogs WHERE tool_name = ?",
        rusqlite::params![tool_name],
    ).map_err(|e| AppError::Storage(e.to_string()))?;

    conn.execute(
        "DELETE FROM bm25_optimized_options WHERE tool_name = ?",
        rusqlite::params![tool_name],
    ).map_err(|e| AppError::Storage(e.to_string()))?;

    Ok(())
}

fn save_tool_bm25_data(
    conn: &rusqlite::Connection,
    catalog: &ToolCatalog,
) -> Result<(), AppError> {
    let (tool_desc, tool_kws) = build_bm25_optimized_data(&catalog.description, &catalog.keywords);

    conn.execute(
        "INSERT INTO bm25_optimized_catalogs (tool_name, preprocessed_description, preprocessed_keywords) VALUES (?, ?, ?)",
        rusqlite::params![catalog.tool_name, tool_desc, tool_kws],
    ).map_err(|e| AppError::Storage(e.to_string()))?;

    Ok(())
}

fn save_options_bm25_data(
    conn: &rusqlite::Connection,
    catalog: &ToolCatalog,
) -> Result<(), AppError> {
    let mut opt_stmt = conn.prepare(
        "INSERT INTO bm25_optimized_options (tool_name, option_name, preprocessed_description, preprocessed_keywords) VALUES (?, ?, ?, ?)"
    ).map_err(|e| AppError::Storage(e.to_string()))?;

    for opt in &catalog.options {
        let (opt_desc, opt_kws) = build_bm25_optimized_data(&opt.description, &opt.keywords);

        opt_stmt.execute(rusqlite::params![
            catalog.tool_name,
            opt.option_name,
            opt_desc,
            opt_kws,
        ]).map_err(|e| AppError::Storage(e.to_string()))?;
    }

    Ok(())
}

impl MatchingStrategyPort for KeywordMatchingEngine {
    fn calculate_similarities(
        &self,
        query: &UserQuery,
    ) -> Result<Vec<Vec<ScoredCandidate>>, AppError> {
        // 1. Ensure engine is loaded
        {
            let state_read = self.index_state.read().map_err(|e| AppError::Matching(format!("Lock poisoned: {}", e)))?;
            if state_read.is_none() {
                drop(state_read);
                self.load_engines()?;
            }
        }

        let state_guard = self.index_state.read().map_err(|e| AppError::Matching(format!("Lock poisoned: {}", e)))?;
        let state = state_guard.as_ref().ok_or_else(|| AppError::Matching("Engine state uninitialized".to_string()))?;

        // 2. Preprocess query
        let query_str = preprocess_query_string(&query.query, &state.catalog_keywords);
        if query_str.trim().is_empty() {
            return Ok(vec![vec![]]);
        }

        // 3. Search
        let searcher = state.reader.searcher();
        let parsed_query = state.query_parser.parse_query(&query_str)
            .map_err(|e| AppError::Matching(format!("Query parsing failed: {}", e)))?;

        let top_docs = searcher.search(&parsed_query, &TopDocs::with_limit(10).order_by_score())
            .map_err(|e| AppError::Matching(format!("Search execution failed: {}", e)))?;

        // 4. Map top docs to candidates
        let candidates = map_search_results_to_candidates(top_docs, &searcher, state)?;

        // 5. Apply Otsu Cutoff dynamic filtering
        let cutoff_config = crate::adapters::matching::otsu::OtsuCutoffConfig::new(0.60, 0.0, 1.0);
        let filtered_candidates = crate::adapters::matching::otsu::apply_otsu_cutoff(candidates, &cutoff_config);

        Ok(vec![filtered_candidates])
    }

    fn load_engines(&self) -> Result<bool, AppError> {
        let conn = rusqlite::Connection::open("local_assistant.db")
            .map_err(|e| AppError::Storage(format!("Failed to open DB for BM25: {}", e)))?;
        let _ = conn.execute("PRAGMA busy_timeout = 5000;", []);

        // 1. Fetch keywords to build bypass set
        let keyword_set = fetch_catalog_keywords(&conn)?;

        // 2. Define schema
        let mut schema_builder = Schema::builder();
        let description_field = schema_builder.add_text_field("description", TEXT);
        let keywords_field = schema_builder.add_text_field("keywords", TEXT);
        let option_name_field = schema_builder.add_text_field("option_name", STRING | STORED);
        let tool_name_field = schema_builder.add_text_field("tool_name", STRING | STORED);
        let raw_description_field = schema_builder.add_text_field("raw_description", STORED);
        let user_friendly_description_field = schema_builder.add_text_field("user_friendly_description", STORED);
        let raw_keywords_field = schema_builder.add_text_field("raw_keywords", STORED);
        let schema = schema_builder.build();

        // 3. Create index in RAM
        let index = Index::create_in_ram(schema.clone());

        // 4. Ingest documents
        let mut index_writer = index.writer(50_000_000)
            .map_err(|e| AppError::Matching(format!("Failed to create IndexWriter: {}", e)))?;

        let fields = TantivyFields {
            description_field,
            keywords_field,
            option_name_field,
            tool_name_field,
            raw_description_field,
            user_friendly_description_field,
            raw_keywords_field,
        };

        ingest_documents(&conn, &mut index_writer, &fields)?;

        index_writer.commit().map_err(|e| AppError::Matching(format!("Commit failed: {}", e)))?;

        // 5. Create reader and query parser
        let reader = index.reader_builder()
            .reload_policy(tantivy::ReloadPolicy::OnCommitWithDelay)
            .try_into()
            .map_err(|e| AppError::Matching(format!("Reader creation failed: {}", e)))?;

        let mut query_parser = QueryParser::for_index(&index, vec![description_field, keywords_field]);
        query_parser.set_field_boost(keywords_field, 5.0);

        let new_state = TantivyIndexState {
            reader,
            query_parser,
            schema,
            description_field,
            keywords_field,
            option_name_field,
            tool_name_field,
            raw_description_field,
            user_friendly_description_field,
            raw_keywords_field,
            catalog_keywords: keyword_set,
        };

        let mut state_guard = self.index_state.write().map_err(|e| AppError::Matching(format!("Lock poisoned: {}", e)))?;
        *state_guard = Some(new_state);
        Ok(true)
    }

    fn create_optimized_catalog(
        &self,
        catalog: &ToolCatalog,
    ) -> Result<(), AppError> {
        let conn = rusqlite::Connection::open("local_assistant.db")
            .map_err(|e| AppError::Storage(format!("Failed to open DB: {}", e)))?;

        ensure_bm25_tables_exist(&conn)?;
        clear_existing_bm25_records(&conn, &catalog.tool_name)?;
        save_tool_bm25_data(&conn, catalog)?;
        save_options_bm25_data(&conn, catalog)?;

        Ok(())
    }

    fn update_optimized_catalog(
        &self,
        catalog: &ToolCatalog,
    ) -> Result<(), AppError> {
        // Since we clear existing and re-insert, update is identical to create
        self.create_optimized_catalog(catalog)
    }

    fn delete_optimized_catalog(
        &self,
        tool_name: &str,
    ) -> Result<(), AppError> {
        let conn = rusqlite::Connection::open("local_assistant.db")
            .map_err(|e| AppError::Storage(format!("Failed to open DB: {}", e)))?;

        // Drop the records directly
        conn.execute(
            "DELETE FROM bm25_optimized_catalogs WHERE tool_name = ?",
            rusqlite::params![tool_name],
        ).map_err(|e| AppError::Storage(e.to_string()))?;

        conn.execute(
            "DELETE FROM bm25_optimized_options WHERE tool_name = ?",
            rusqlite::params![tool_name],
        ).map_err(|e| AppError::Storage(e.to_string()))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::models::CommandRules;

    #[test]
    fn test_build_bm25_optimized_data() {
        let description = "Copy -R directories dynamically";
        let keywords = "find search";
        let (desc_str, key_str) = build_bm25_optimized_data(description, keywords);

        // "copy" is in stop-words so it is removed, Porter stemming applies to "directories" -> "directori", "dynamically" -> "dynam"
        // "-R" becomes "-r"
        assert_eq!(desc_str, "-r directori dynam");

        assert!(key_str.contains("find"));
        assert!(key_str.contains("search"));
        // Shingles (stemmed): "copi-r", "-rdirectori", "copi-rdirectori"
        assert!(key_str.contains("copi-r"));
        assert!(key_str.contains("-rdirectori"));
    }

    #[test]
    fn test_create_optimized_catalog_keyword() {
        use crate::ports::outbound::storage::StoragePort;
        use crate::adapters::persistence::PersistenceAdapter;

        let storage = PersistenceAdapter::new();
        let engine = KeywordMatchingEngine::new();
        let tool_name = "test_keyword_db_tool";
        let _ = storage.delete_catalog(tool_name);
        let _ = engine.delete_optimized_catalog(tool_name);

        let catalog = ToolCatalog {
            tool_name: tool_name.to_string(),
            description: "Copy -R directories".to_string(),
            user_friendly_description: "search".to_string(),
            keywords: "find search".to_string(),
            version: "1.0".to_string(),
            options: vec![
                CommandOption {
                    option_name: "-R".to_string(),
                    description: "Recursive copy directories".to_string(),
                    user_friendly_description: "recursive".to_string(),
                    keywords: "recursive".to_string(),
                }
            ],
            rules: CommandRules(serde_json::json!({})),
        };

        // 1. Ingest base catalog
        storage.save_catalog(&catalog).unwrap();

        // 2. Run engine create
        engine.create_optimized_catalog(&catalog).unwrap();

        // 3. Query custom database tables directly to verify content
        let conn = rusqlite::Connection::open("local_assistant.db").unwrap();
        let mut stmt1 = conn.prepare("SELECT preprocessed_description, preprocessed_keywords FROM bm25_optimized_catalogs WHERE tool_name = ?").unwrap();
        let mut row1 = stmt1.query(rusqlite::params![tool_name]).unwrap();
        let r1 = row1.next().unwrap().unwrap();
        let parent_desc: String = r1.get(0).unwrap();
        let parent_kws: String = r1.get(1).unwrap();
        assert_eq!(parent_desc, "-r directori");
        assert!(parent_kws.contains("find"));

        let mut stmt2 = conn.prepare("SELECT option_name, preprocessed_description, preprocessed_keywords FROM bm25_optimized_options WHERE tool_name = ?").unwrap();
        let mut row2 = stmt2.query(rusqlite::params![tool_name]).unwrap();
        let r2 = row2.next().unwrap().unwrap();
        let opt_name: String = r2.get(0).unwrap();
        let opt_desc: String = r2.get(1).unwrap();
        assert_eq!(opt_name, "-R");
        assert_eq!(opt_desc, "recurs directori");

        // Clean up
        let _ = storage.delete_catalog(tool_name);
        let _ = engine.delete_optimized_catalog(tool_name);
    }

    #[test]
    fn test_calculate_similarities_integration() {
        use crate::ports::outbound::storage::StoragePort;
        use crate::adapters::persistence::PersistenceAdapter;
        use crate::core::models::{UserQuery, CommandOption, ToolCatalog};

        let storage = PersistenceAdapter::new();
        let engine = KeywordMatchingEngine::new();

        // 1. Create a dummy tool catalog
        let tool_name = "test_dummy_keyword_tool_unique_12345";
        let catalog = ToolCatalog {
            tool_name: tool_name.to_string(),
            description: "A dummy tool for testing keyword search".to_string(),
            user_friendly_description: "dummy tool".to_string(),
            keywords: "dummy test search".to_string(),
            version: "1.0".to_string(),
            options: vec![
                CommandOption {
                    option_name: "--test-flag".to_string(),
                    description: "Enable the secret integration testing mode".to_string(),
                    user_friendly_description: "test flag".to_string(),
                    keywords: "secret integration flag".to_string(),
                }
            ],
            rules: CommandRules(serde_json::json!({})),
        };

        // 2. Delete if already exists (just in case)
        let _ = storage.delete_catalog(tool_name);
        let _ = engine.delete_optimized_catalog(tool_name);

        // 3. Save base catalog and then optimize it using keyword engine
        storage.save_catalog(&catalog).unwrap();
        engine.create_optimized_catalog(&catalog).unwrap();

        // 4. Load engines to reload the Tantivy index from the DB
        let load_res = engine.load_engines().unwrap();
        assert!(load_res);

        // 5. Query the engine
        let query = UserQuery {
            query: "secret integration".to_string(),
            n_grams: None,
        };
        let results = engine.calculate_similarities(&query).unwrap();
        
        // Cleanup first before assertions so we don't leave garbage on failure
        let _ = storage.delete_catalog(tool_name);
        let _ = engine.delete_optimized_catalog(tool_name);

        // 6. Assertions
        assert_eq!(results.len(), 1);
        let candidates = &results[0];
        assert!(!candidates.is_empty(), "Candidates should not be empty");
        
        // Find our option in the candidates
        let candidate = candidates.iter().find(|c| c.option.option_name == "--test-flag");
        assert!(candidate.is_some(), "Should find '--test-flag' in search results");
        let candidate = candidate.unwrap();
        assert!(candidate.score > 0.0);
        assert_eq!(candidate.option.description, "Enable the secret integration testing mode");
    }

    #[test]
    fn test_preprocess_query_string() {
        let mut bypass = HashSet::new();
        bypass.insert("search".to_string());
        let result = preprocess_query_string("Copying -R directories search", &bypass);
        // "copying" -> "copi" (stemmed), "-R" -> "-r", "directories" -> "directori" (stemmed)
        // "search" is bypassed (not filtered as stop-word, even though "search" isn't a standard stopword anyway, but bypass logic is tested)
        // Shingles: "copi-r", "-rdirectori", "copi-rdirectori"
        assert!(result.contains("\\-r"));
        assert!(result.contains("directori"));
        assert!(result.contains("copying\\-r"));
        assert!(result.contains("\\-rdirectories"));
    }
}

