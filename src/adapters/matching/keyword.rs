use crate::ports::outbound::matching_strategy::MatchingStrategyPort;
use crate::core::errors::AppError;
use crate::core::models::{UserQuery, ScoredCandidate, CommandOption, ToolCatalog, OptimizedToolCatalog, OptimizedCommandOption, OptimizedData};

/// Outbound adapter representing the keyword-based matching engine.
#[derive(Clone, Copy)]
pub struct KeywordMatchingEngine;

impl KeywordMatchingEngine {
    /// Creates a new KeywordMatchingEngine instance.
    pub fn new() -> Self {
        Self
    }
}

impl Default for KeywordMatchingEngine {
    fn default() -> Self {
        Self::new()
    }
}

fn preprocess_description(desc: &str) -> String {
    desc.replace('\'', "")
        .replace('’', "")
        .replace('/', " ")
        .replace(':', " ")
}

fn generate_shingles(text: &str) -> Vec<String> {
    // 1. Lowercase
    let lowered = text.to_lowercase();
    // 2. Normalize: strip apostrophes
    let normalized = lowered.replace('\'', "").replace('’', "");
    // 3. Normalize: replace '/' and ':' with spaces
    let normalized = normalized.replace('/', " ").replace(':', " ");
    
    // 4. Split by whitespace and clean each word (keep alphanumeric and hyphens)
    let words: Vec<String> = normalized
        .split_whitespace()
        .map(|word| {
            word.chars()
                .filter(|c| c.is_alphanumeric() || *c == '-')
                .collect::<String>()
        })
        .filter(|w| !w.is_empty())
        .collect();

    let mut shingles = Vec::new();
    
    // Generate Bi-grams
    for window in words.windows(2) {
        shingles.push(format!("{}{}", window[0], window[1]));
    }
    
    // Generate Tri-grams
    for window in words.windows(3) {
        shingles.push(format!("{}{}{}", window[0], window[1], window[2]));
    }
    
    shingles
}

impl MatchingStrategyPort for KeywordMatchingEngine {
    fn calculate_similarities(
        &self,
        query: &UserQuery,
    ) -> Result<Vec<Vec<ScoredCandidate>>, AppError> {
        // Return a dummy matched option from the keyword engine
        Ok(vec![vec![ScoredCandidate {
            option: CommandOption {
                option_name: "-la".to_string(),
                description: format!("Keyword match result for: {}", query.query),
                user_friendly_description: "".to_string(),
                keywords: "keyword".to_string(),
            },
            score: 0.85,
        }]])
    }

    fn load_engines(&self) -> Result<bool, AppError> {
        Ok(true)
    }

    fn optimize_catalog(
        &self,
        catalog: &ToolCatalog,
    ) -> Result<OptimizedToolCatalog, AppError> {
        let tool_preprocessed_desc = preprocess_description(&catalog.description);
        let tool_shingles = generate_shingles(&catalog.description);
        let tool_preprocessed_keywords = if tool_shingles.is_empty() {
            catalog.keywords.clone()
        } else {
            format!("{} {}", catalog.keywords, tool_shingles.join(" "))
        };

        let tool_optimized_data = vec![
            OptimizedData {
                key: "bm25_preprocessed_description".to_string(),
                data: tool_preprocessed_desc.into_bytes(),
                data_type: "TEXT".to_string(),
            },
            OptimizedData {
                key: "bm25_preprocessed_keywords".to_string(),
                data: tool_preprocessed_keywords.into_bytes(),
                data_type: "TEXT".to_string(),
            },
        ];

        let mut options = Vec::new();
        for opt in &catalog.options {
            let opt_preprocessed_desc = preprocess_description(&opt.description);
            let opt_shingles = generate_shingles(&opt.description);
            let opt_preprocessed_keywords = if opt_shingles.is_empty() {
                opt.keywords.clone()
            } else {
                format!("{} {}", opt.keywords, opt_shingles.join(" "))
            };

            let opt_optimized_data = vec![
                OptimizedData {
                    key: "bm25_preprocessed_description".to_string(),
                    data: opt_preprocessed_desc.into_bytes(),
                    data_type: "TEXT".to_string(),
                },
                OptimizedData {
                    key: "bm25_preprocessed_keywords".to_string(),
                    data: opt_preprocessed_keywords.into_bytes(),
                    data_type: "TEXT".to_string(),
                },
            ];

            options.push(OptimizedCommandOption {
                option_name: opt.option_name.clone(),
                description: opt.description.clone(),
                user_friendly_description: opt.user_friendly_description.clone(),
                keywords: opt.keywords.clone(),
                optimized_data: opt_optimized_data,
            });
        }

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
