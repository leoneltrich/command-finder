use crate::ports::outbound::matching_strategy::MatchingStrategyPort;
use crate::core::errors::AppError;
use crate::core::models::{UserQuery, ScoredCandidate, CommandOption, ToolCatalog, OptimizedToolCatalog, OptimizedCommandOption, OptimizedData};
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

fn build_bm25_optimized_data(description: &str, keywords: &str) -> Vec<OptimizedData> {
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

    vec![
        OptimizedData {
            key: "bm25_preprocessed_description".to_string(),
            data: preprocessed_desc.into_bytes(),
            data_type: "TEXT".to_string(),
        },
        OptimizedData {
            key: "bm25_preprocessed_keywords".to_string(),
            data: preprocessed_keywords.into_bytes(),
            data_type: "TEXT".to_string(),
        },
    ]
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

fn check_dynamic_columns_exist(conn: &rusqlite::Connection) -> bool {
    if let Ok(mut stmt) = conn.prepare("PRAGMA table_info(command_options)") {
        if let Ok(mut rows) = stmt.query([]) {
            while let Some(row) = rows.next().unwrap_or(None) {
                if let Ok(name) = row.get::<_, String>(1) {
                    if name == "bm25_preprocessed_description" {
                        return true;
                    }
                }
            }
        }
    }
    false
}

fn ingest_documents(
    conn: &rusqlite::Connection,
    index_writer: &mut tantivy::IndexWriter,
    state_fields: &TantivyFields,
) -> Result<(), AppError> {
    if !check_dynamic_columns_exist(conn) {
        return Ok(());
    }

    let mut stmt = conn.prepare(
        "SELECT tool_name, option_name, description, user_friendly_description, keywords, bm25_preprocessed_description, bm25_preprocessed_keywords FROM command_options"
    ).map_err(|e| AppError::Storage(e.to_string()))?;
    let mut rows = stmt.query([]).map_err(|e| AppError::Storage(e.to_string()))?;
    while let Some(row) = rows.next().map_err(|e| AppError::Storage(e.to_string()))? {
        let tool_name: String = row.get(0).map_err(|e| AppError::Storage(e.to_string()))?;
        let option_name: String = row.get(1).map_err(|e| AppError::Storage(e.to_string()))?;
        let raw_desc: String = row.get(2).map_err(|e| AppError::Storage(e.to_string()))?;
        let raw_user_desc: String = row.get(3).map_err(|e| AppError::Storage(e.to_string()))?;
        let raw_kws: String = row.get(4).map_err(|e| AppError::Storage(e.to_string()))?;
        let preprocessed_desc_bytes: Option<Vec<u8>> = row.get(5).map_err(|e| AppError::Storage(e.to_string()))?;
        let preprocessed_kws_bytes: Option<Vec<u8>> = row.get(6).map_err(|e| AppError::Storage(e.to_string()))?;
        let preprocessed_desc = preprocessed_desc_bytes.and_then(|b| String::from_utf8(b).ok());
        let preprocessed_kws = preprocessed_kws_bytes.and_then(|b| String::from_utf8(b).ok());

        if let (Some(desc), Some(kw)) = (preprocessed_desc, preprocessed_kws) {
            let doc = doc!(
                state_fields.tool_name_field => tool_name,
                state_fields.option_name_field => option_name,
                state_fields.raw_description_field => raw_desc,
                state_fields.user_friendly_description_field => raw_user_desc,
                state_fields.raw_keywords_field => raw_kws,
                state_fields.description_field => desc,
                state_fields.keywords_field => kw
            );
            index_writer.add_document(doc).map_err(|e| AppError::Matching(e.to_string()))?;
        }
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

    fn optimize_catalog(
        &self,
        catalog: &ToolCatalog,
    ) -> Result<OptimizedToolCatalog, AppError> {
        let tool_optimized_data = build_bm25_optimized_data(&catalog.description, &catalog.keywords);

        let options = catalog.options.iter().map(|opt| {
            OptimizedCommandOption {
                option_name: opt.option_name.clone(),
                description: opt.description.clone(),
                user_friendly_description: opt.user_friendly_description.clone(),
                keywords: opt.keywords.clone(),
                optimized_data: build_bm25_optimized_data(&opt.description, &opt.keywords),
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
            optimized_data: tool_optimized_data,
        })
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
        let opt_data = build_bm25_optimized_data(description, keywords);

        let desc_data = opt_data.iter().find(|d| d.key == "bm25_preprocessed_description").unwrap();
        let desc_str = String::from_utf8(desc_data.data.clone()).unwrap();
        // "copy" is in stop-words so it is removed, Porter stemming applies to "directories" -> "directori", "dynamically" -> "dynam"
        // "-R" becomes "-r"
        assert_eq!(desc_str, "-r directori dynam");

        let key_data = opt_data.iter().find(|d| d.key == "bm25_preprocessed_keywords").unwrap();
        let key_str = String::from_utf8(key_data.data.clone()).unwrap();
        assert!(key_str.contains("find"));
        assert!(key_str.contains("search"));
        // Shingles (stemmed): "copi-r", "-rdirectori", "copi-rdirectori"
        assert!(key_str.contains("copi-r"));
        assert!(key_str.contains("-rdirectori"));
    }

    #[test]
    fn test_optimize_catalog_keyword() {
        let engine = KeywordMatchingEngine::new();
        let catalog = ToolCatalog {
            tool_name: "test_tool".to_string(),
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

        let result = engine.optimize_catalog(&catalog).unwrap();
        
        // Verify parent optimized data
        let opt_desc = result.optimized_data.iter().find(|d| d.key == "bm25_preprocessed_description").unwrap();
        let opt_desc_str = String::from_utf8(opt_desc.data.clone()).unwrap();
        // "copy" is stop word, so removed
        assert_eq!(opt_desc_str, "-r directori");

        // Verify child options optimized data
        assert_eq!(result.options.len(), 1);
        let opt_option = &result.options[0];
        let opt_opt_desc = opt_option.optimized_data.iter().find(|d| d.key == "bm25_preprocessed_description").unwrap();
        let opt_opt_desc_str = String::from_utf8(opt_opt_desc.data.clone()).unwrap();
        // "copy" is stop word, so "recursive copy directories" -> "recurs directori"
        assert_eq!(opt_opt_desc_str, "recurs directori");
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

        // 3. Optimize and save
        let optimized = engine.optimize_catalog(&catalog).unwrap();
        let save_res = storage.save_catalog(&optimized).unwrap();
        assert!(save_res);

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

