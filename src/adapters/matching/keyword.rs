use crate::ports::outbound::matching_strategy::MatchingStrategyPort;
use crate::core::errors::AppError;
use crate::core::models::{UserQuery, ScoredCandidate, CommandOption, ToolCatalog, OptimizedToolCatalog, OptimizedCommandOption, OptimizedData};
use std::collections::HashSet;
use rust_stemmers::{Algorithm, Stemmer};

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

fn build_bm25_optimized_data(description: &str, keywords: &str) -> Vec<OptimizedData> {
    let keyword_set: HashSet<String> = keywords
        .to_lowercase()
        .split_whitespace()
        .map(|w| w.chars().filter(|c| c.is_alphanumeric() || *c == '-').collect())
        .collect();

    let english_stops: HashSet<String> = stop_words::get(stop_words::LANGUAGE::English)
        .iter()
        .map(|s| s.to_string())
        .collect();

    let stemmer = Stemmer::create(Algorithm::English);

    let desc_tokens: Vec<String> = description
        .to_lowercase()
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
        .filter(|w| !english_stops.contains(w) || keyword_set.contains(w))
        .map(|w| stemmer.stem(&w).into_owned())
        .collect();

    let preprocessed_desc = desc_tokens.join(" ");

    let raw_desc_tokens: Vec<String> = description
        .to_lowercase()
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
        .map(|w| stemmer.stem(&w).into_owned())
        .collect();

    let mut shingles = Vec::new();
    for window in raw_desc_tokens.windows(2) {
        shingles.push(format!("{}{}", window[0], window[1]));
    }
    for window in raw_desc_tokens.windows(3) {
        shingles.push(format!("{}{}{}", window[0], window[1], window[2]));
    }

    let keyword_tokens: Vec<String> = keywords
        .to_lowercase()
        .split_whitespace()
        .map(|word| {
            word.chars()
                .filter(|c| c.is_alphanumeric() || *c == '-')
                .collect::<String>()
        })
        .filter(|w| !w.is_empty())
        .map(|w| stemmer.stem(&w).into_owned())
        .collect();

    let mut final_keywords = keyword_tokens;
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
}

