use crate::core::models::{ScoredCandidate, ScoredTool, ToolCatalog, CommandOption};
use crate::core::errors::AppError;
use std::collections::HashMap;

/// Core component responsible for rank and similarity score aggregation.
/// Implements NFR-8 by combining similarity rankings from multiple engines.
pub struct SimilarityRankAggregator;

impl SimilarityRankAggregator {
    /// Creates a new SimilarityRankAggregator.
    pub fn new() -> Self {
        Self
    }

    /// Aggregates list of similarity results from multiple matching engines into a single ranking.
    pub fn aggregate(
        &self,
        engine_results: &[Vec<Vec<ScoredCandidate>>],
    ) -> Result<Vec<Vec<ScoredCandidate>>, AppError> {
        println!("[SimilarityRankAggregator] Aggregating results from {} engines.", engine_results.len());
        
        let mut merged_results: Vec<Vec<ScoredCandidate>> = Vec::new();
        for engine_result in engine_results {
            for (i, candidate_group) in engine_result.iter().enumerate() {
                if merged_results.len() <= i {
                    merged_results.push(candidate_group.clone());
                } else {
                    merged_results[i].extend(candidate_group.clone());
                }
            }
        }

        // Sort each candidate group by score descending
        for group in &mut merged_results {
            group.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        }

        Ok(merged_results)
    }

    /// Generic rank aggregation helper that performs min-max normalization, 
    /// score summation using engine weights, and sorting.
    fn aggregate_generic<T, F>(
        &self,
        engine_results: &[(&[T], f64)],
        get_key: F,
    ) -> Result<Vec<T>, AppError>
    where
        T: crate::adapters::matching::otsu::Scored + Clone,
        F: Fn(&T) -> String,
    {
        let mut aggregated_scores: HashMap<String, (T, f64)> = HashMap::new();

        for &(results, weight) in engine_results {
            if results.is_empty() {
                continue;
            }

            // 1. Find the maximum score for min-max normalization
            let mut max_score = 0.0f64;
            for r in results {
                if r.score() > max_score {
                    max_score = r.score();
                }
            }

            // 2. Accumulate normalized, weighted score for each item
            for r in results {
                let normalized_score = if max_score > 0.0 {
                    r.score() / max_score
                } else {
                    0.0
                };

                let key = get_key(r);
                let entry = aggregated_scores.entry(key).or_insert_with(|| {
                    (r.clone(), 0.0)
                });
                entry.1 += normalized_score * weight;
            }
        }

        // Convert map to Vec and sort descending by aggregated score
        let mut final_results: Vec<T> = aggregated_scores
            .into_values()
            .map(|(mut item, score)| {
                item.set_score(score);
                item
            })
            .collect();

        final_results.sort_by(|a, b| b.score().partial_cmp(&a.score()).unwrap_or(std::cmp::Ordering::Equal));

        Ok(final_results)
    }

    /// Aggregates tools list with min-max normalization (min = 0) and weighted score summation.
    pub fn aggregate_tools(
        &self,
        engine_results: &[(&[ScoredTool], f64)],
    ) -> Result<Vec<ScoredTool>, AppError> {
        self.aggregate_generic(engine_results, |st| st.tool.tool_name.clone())
    }

    /// Aggregates options list with min-max normalization, weighted score summation,
    /// and applies an Otsu cutoff on the final ranked candidates.
    pub fn aggregate_options(
        &self,
        engine_results: &[(&[ScoredCandidate], f64)],
        otsu_config: &crate::adapters::matching::otsu::OtsuCutoffConfig,
    ) -> Result<Vec<ScoredCandidate>, AppError> {
        let ranked = self.aggregate_generic(engine_results, |sc| sc.option.option_name.clone())?;
        let filtered = crate::adapters::matching::otsu::apply_otsu_cutoff(ranked, otsu_config);
        Ok(filtered)
    }
}

impl Default for SimilarityRankAggregator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::models::CommandRules;

    fn create_dummy_tool(name: &str) -> ToolCatalog {
        ToolCatalog {
            tool_name: name.to_string(),
            description: "test".to_string(),
            user_friendly_description: "test".to_string(),
            keywords: "test".to_string(),
            version: "1.0".to_string(),
            options: vec![],
            rules: CommandRules(serde_json::Value::Null),
        }
    }

    #[test]
    fn test_aggregate_tools_normalization_and_weights() {
        let aggregator = SimilarityRankAggregator::new();

        let tool_a = create_dummy_tool("tool_a");
        let tool_b = create_dummy_tool("tool_b");

        // Engine 1: max score = 50.0. tool_a = 50.0 (norm = 1.0), tool_b = 25.0 (norm = 0.5)
        let results_1 = vec![
            ScoredTool { tool: tool_a.clone(), score: 50.0 },
            ScoredTool { tool: tool_b.clone(), score: 25.0 },
        ];
        let weight_1 = 2.0;

        // Engine 2: max score = 1.0. tool_b = 1.0 (norm = 1.0)
        // tool_a is missing (should get score 0.0)
        let results_2 = vec![
            ScoredTool { tool: tool_b.clone(), score: 1.0 },
        ];
        let weight_2 = 1.0;

        // Aggregation inputs
        let inputs = vec![
            (results_1.as_slice(), weight_1),
            (results_2.as_slice(), weight_2),
        ];

        let aggregated = aggregator.aggregate_tools(&inputs).unwrap();

        // Expectations:
        // tool_a = (1.0 * 2.0) + (0.0 * 1.0) = 2.0
        // tool_b = (0.5 * 2.0) + (1.0 * 1.0) = 2.0
        assert_eq!(aggregated.len(), 2);
        
        let a_res = aggregated.iter().find(|t| t.tool.tool_name == "tool_a").unwrap();
        let b_res = aggregated.iter().find(|t| t.tool.tool_name == "tool_b").unwrap();
        
        assert_eq!(a_res.score, 2.0);
        assert_eq!(b_res.score, 2.0);
    }

    #[test]
    fn test_aggregate_tools_sorting_and_zero_scores() {
        let aggregator = SimilarityRankAggregator::new();

        let tool_a = create_dummy_tool("tool_a");
        let tool_b = create_dummy_tool("tool_b");

        // Engine results with 0.0 scores
        let results = vec![
            ScoredTool { tool: tool_a.clone(), score: 0.0 },
            ScoredTool { tool: tool_b.clone(), score: 0.0 },
        ];
        let inputs = vec![
            (results.as_slice(), 1.0),
        ];

        let aggregated = aggregator.aggregate_tools(&inputs).unwrap();
        assert_eq!(aggregated.len(), 2);
        assert_eq!(aggregated[0].score, 0.0);
        assert_eq!(aggregated[1].score, 0.0);
    }

    fn create_dummy_option(name: &str) -> CommandOption {
        CommandOption {
            option_name: name.to_string(),
            description: "test".to_string(),
            user_friendly_description: "test".to_string(),
            keywords: "test".to_string(),
        }
    }

    #[test]
    fn test_aggregate_options_normalization_weights_and_otsu() {
        let aggregator = SimilarityRankAggregator::new();

        let opt_a = create_dummy_option("opt_a");
        let opt_b = create_dummy_option("opt_b");

        // Engine 1: opt_a = 10.0 (norm = 1.0), opt_b = 5.0 (norm = 0.5)
        let results_1 = vec![
            ScoredCandidate { option: opt_a.clone(), score: 10.0 },
            ScoredCandidate { option: opt_b.clone(), score: 5.0 },
        ];
        let weight_1 = 1.0;

        let inputs = vec![
            (results_1.as_slice(), weight_1),
        ];

        // Using Otsu cutoff config: alpha = 0.60
        // opt_a score = 1.0. opt_b score = 0.5.
        // since opt_b score (0.5) < rel_floor (0.6 * 1.0 = 0.6), opt_b score gets zeroed.
        // then opt_b is discarded by the otsu cutoff retain step.
        let otsu_config = crate::adapters::matching::otsu::OtsuCutoffConfig::new(0.60, 0.0, 1.0);
        let aggregated = aggregator.aggregate_options(&inputs, &otsu_config).unwrap();

        assert_eq!(aggregated.len(), 1);
        assert_eq!(aggregated[0].option.option_name, "opt_a");
        assert_eq!(aggregated[0].score, 1.0);
    }
}
