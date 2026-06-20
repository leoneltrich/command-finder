use crate::core::models::{ScoredCandidate, ScoredTool};
use std::collections::HashSet;

pub trait Scored {
    fn score(&self) -> f64;
    fn set_score(&mut self, score: f64);
}

impl Scored for ScoredCandidate {
    fn score(&self) -> f64 {
        self.score
    }
    fn set_score(&mut self, score: f64) {
        self.score = score;
    }
}

impl Scored for ScoredTool {
    fn score(&self) -> f64 {
        self.score
    }
    fn set_score(&mut self, score: f64) {
        self.score = score;
    }
}

#[derive(Debug, Clone)]
pub struct OtsuCutoffConfig {
    pub alpha: f64,
    pub hard_floor: f64,
    pub multiplier: f64,
}

impl OtsuCutoffConfig {
    pub fn new(alpha: f64, hard_floor: f64, multiplier: f64) -> Self {
        Self {
            alpha,
            hard_floor,
            multiplier,
        }
    }
}

impl Default for OtsuCutoffConfig {
    fn default() -> Self {
        Self {
            alpha: 0.60,
            hard_floor: 0.0,
            multiplier: 1.0,
        }
    }
}

pub fn compute_otsu_threshold(drops: &[f64]) -> f64 {
    if drops.is_empty() {
        return 0.0;
    }

    // Deduplicate and sort drop values to establish bin thresholds
    let mut unique_drops = HashSet::new();
    for &d in drops {
        if !d.is_nan() {
            unique_drops.insert(d.to_bits());
        }
    }

    let mut unique_values: Vec<f64> = unique_drops.into_iter()
        .map(f64::from_bits)
        .collect();

    unique_values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let mut max_variance = -1.0;
    let mut best_t = 0.0;
    let total_count = drops.len() as f64;

    // Evaluate each drop value as a potential threshold partition
    for &t in &unique_values {
        let mut count_0 = 0.0;
        let mut sum_0 = 0.0;
        let mut count_1 = 0.0;
        let mut sum_1 = 0.0;

        for &r in drops {
            if r <= t {
                count_0 += 1.0;
                sum_0 += r;
            } else {
                count_1 += 1.0;
                sum_1 += r;
            }
        }

        let w_0 = count_0 / total_count;
        let w_1 = count_1 / total_count;

        let mu_0 = if count_0 > 0.0 { sum_0 / count_0 } else { 0.0 };
        let mu_1 = if count_1 > 0.0 { sum_1 / count_1 } else { 0.0 };

        // Between-class variance formula
        let variance = w_0 * w_1 * (mu_0 - mu_1) * (mu_0 - mu_1);

        if variance > max_variance {
            max_variance = variance;
            best_t = t;
        }
    }

    best_t
}

pub fn apply_otsu_cutoff<T: Scored + Clone>(
    mut candidates: Vec<T>,
    config: &OtsuCutoffConfig,
) -> Vec<T> {
    if candidates.is_empty() {
        return candidates;
    }

    // 1. Apply score floors (zero out weak matches)
    let max_score = candidates.first().map(|c| c.score()).unwrap_or(0.0);
    let rel_floor = max_score * config.alpha;

    for c in &mut candidates {
        if c.score() < rel_floor || c.score() < config.hard_floor {
            c.set_score(0.0);
        }
    }

    // 2. Compute adjacent relative drops
    let mut drops = Vec::new();
    for j in 0..candidates.len().saturating_sub(1) {
        let s_j = candidates[j].score();
        let s_next = candidates[j+1].score();
        let drop = if s_j.abs() > 1e-6 { (s_j - s_next) / s_j } else { 0.0 };
        drops.push(drop);
    }

    // 3. Compute Otsu threshold and apply safety multiplier
    let theta_otsu = compute_otsu_threshold(&drops);
    let theta = config.multiplier * theta_otsu;

    // 4. Traverse drops to find the first "cliff"
    let mut found_cliff = false;
    let mut cliff_idx = 0;

    for j in 0..drops.len() {
        if drops[j] > theta {
            cliff_idx = j;
            found_cliff = true;
            break;
        }
    }

    // 5. Select everything preceding the cliff (or all if no cliff is found)
    if found_cliff {
        candidates.truncate(cliff_idx + 1);
    }

    // Also remove any candidates that were zeroed out by the floors
    candidates.retain(|c| c.score() > 0.0);

    candidates
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::models::CommandOption;

    fn make_candidates(scores: &[f64]) -> Vec<ScoredCandidate> {
        scores.iter().map(|&s| ScoredCandidate {
            option: CommandOption {
                option_name: format!("-f{}", s),
                description: "".to_string(),
                user_friendly_description: "".to_string(),
                keywords: "".to_string(),
            },
            score: s,
        }).collect()
    }

    #[test]
    fn test_otsu_cliff_detection() {
        let scores = vec![10.0, 9.5, 9.0, 3.0, 2.8, 2.5];
        let candidates = make_candidates(&scores);
        let config = OtsuCutoffConfig::new(0.60, 0.0, 1.0);

        let filtered = apply_otsu_cutoff(candidates, &config);
        assert_eq!(filtered.len(), 3);
        assert_eq!(filtered[0].score, 10.0);
        assert_eq!(filtered[1].score, 9.5);
        assert_eq!(filtered[2].score, 9.0);
    }

    #[test]
    fn test_otsu_no_cliff() {
        let scores = vec![10.0, 10.0, 10.0, 10.0];
        let candidates = make_candidates(&scores);
        let config = OtsuCutoffConfig::new(0.60, 0.0, 1.0);

        let filtered = apply_otsu_cutoff(candidates, &config);
        assert_eq!(filtered.len(), 4);
    }
}
