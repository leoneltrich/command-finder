use crate::core::models::{ScoredCandidate, ScoredTool, ToolCatalog};
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

    /// Aggregates tools list with min-max normalization (min = 0) and weighted score summation.
    pub fn aggregate_tools(
        &self,
        engine_results: &[(&[ScoredTool], f64)],
    ) -> Result<Vec<ScoredTool>, AppError> {
        let mut aggregated_scores: HashMap<String, (ToolCatalog, f64)> = HashMap::new();

        for &(results, weight) in engine_results {
            if results.is_empty() {
                continue;
            }

            // 1. Find the maximum score for min-max normalization
            let mut max_score = 0.0f64;
            for r in results {
                if r.score > max_score {
                    max_score = r.score;
                }
            }

            // 2. Accumulate normalized, weighted score for each tool
            for r in results {
                let normalized_score = if max_score > 0.0 {
                    r.score / max_score
                } else {
                    0.0
                };

                let entry = aggregated_scores.entry(r.tool.tool_name.clone()).or_insert_with(|| {
                    (r.tool.clone(), 0.0)
                });
                entry.1 += normalized_score * weight;
            }
        }

        // Convert map to Vec and sort descending by aggregated score
        let mut final_results: Vec<ScoredTool> = aggregated_scores
            .into_values()
            .map(|(tool, score)| ScoredTool { tool, score })
            .collect();

        final_results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

        Ok(final_results)
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
}
