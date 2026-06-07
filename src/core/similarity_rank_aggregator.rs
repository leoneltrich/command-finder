use crate::core::models::ScoredCandidate;
use crate::core::errors::AppError;

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
        
        // Dummy logic: simply merge the results by taking candidates from all engines
        // In a real implementation, this would normalize and mathematically fuse the scores.
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
}

impl Default for SimilarityRankAggregator {
    fn default() -> Self {
        Self::new()
    }
}
