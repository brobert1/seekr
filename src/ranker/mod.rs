//! Hybrid ranking module combining BM25 and semantic search
//!
//! Architecture: We use Reciprocal Rank Fusion (RRF) and weighted linear
//! fusion to combine lexical (BM25) and semantic (vector) search results.
//!
//! Key concepts:
//! - Score normalization: Min-max scaling to [0, 1] range
//! - Linear fusion: α × BM25 + (1-α) × semantic
//! - RRF: 1 / (k + rank) for robust rank aggregation

use std::collections::HashMap;

/// A result from any search source (BM25 or semantic)
#[derive(Debug, Clone)]
pub struct RankedResult {
    pub file_path: String,
    pub score: f32,
    pub source: SearchSource,
    pub start_line: usize,
    pub end_line: usize,
    pub content_preview: String,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchSource {
    Lexical,  // BM25/Tantivy
    Semantic, // Vector/Embedding
    Hybrid,   // Fused result
}

/// Configuration for hybrid ranking
#[derive(Debug, Clone)]
pub struct HybridConfig {
    /// Weight for lexical (BM25) scores. Semantic weight = 1 - alpha
    pub alpha: f32,
    /// RRF constant (typically 60)
    pub rrf_k: f32,
    /// Whether to use RRF (true) or linear fusion (false)  
    pub use_rrf: bool,
}

impl Default for HybridConfig {
    fn default() -> Self {
        Self {
            alpha: 0.5,    // Equal weight to both
            rrf_k: 60.0,   // Standard RRF constant
            use_rrf: true, // RRF is more robust
        }
    }
}

/// Hybrid ranker that fuses BM25 and semantic results
pub struct HybridRanker {
    config: HybridConfig,
}

impl HybridRanker {
    pub fn new(config: HybridConfig) -> Self {
        Self { config }
    }

    /// Fuse lexical and semantic results into a single ranked list
    pub fn fuse(
        &self,
        lexical_results: Vec<RankedResult>,
        semantic_results: Vec<RankedResult>,
        limit: usize,
    ) -> Vec<RankedResult> {
        if self.config.use_rrf {
            self.rrf_fusion(lexical_results, semantic_results, limit)
        } else {
            self.linear_fusion(lexical_results, semantic_results, limit)
        }
    }

    /// Reciprocal Rank Fusion (RRF)
    /// Score = sum of 1 / (k + rank) for each result list
    fn rrf_fusion(
        &self,
        lexical_results: Vec<RankedResult>,
        semantic_results: Vec<RankedResult>,
        limit: usize,
    ) -> Vec<RankedResult> {
        let mut scores: HashMap<String, (f32, Option<RankedResult>)> = HashMap::new();
        let k = self.config.rrf_k;

        // Process lexical results
        for (rank, result) in lexical_results.into_iter().enumerate() {
            let rrf_score = 1.0 / (k + rank as f32 + 1.0);
            let key = result.file_path.clone();

            scores
                .entry(key)
                .and_modify(|(s, r)| {
                    *s += rrf_score * self.config.alpha;
                    if r.is_none() {
                        *r = Some(result.clone());
                    }
                })
                .or_insert((rrf_score * self.config.alpha, Some(result)));
        }

        // Process semantic results
        for (rank, result) in semantic_results.into_iter().enumerate() {
            let rrf_score = 1.0 / (k + rank as f32 + 1.0);
            let key = result.file_path.clone();

            scores
                .entry(key)
                .and_modify(|(s, r)| {
                    *s += rrf_score * (1.0 - self.config.alpha);
                    if r.is_none() {
                        *r = Some(result.clone());
                    }
                })
                .or_insert((rrf_score * (1.0 - self.config.alpha), Some(result)));
        }

        // Sort by fused score and take top results
        let mut results: Vec<_> = scores
            .into_iter()
            .filter_map(|(_, (score, result))| {
                result.map(|mut r| {
                    r.score = score;
                    r.source = SearchSource::Hybrid;
                    r
                })
            })
            .collect();

        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
        results.truncate(limit);
        results
    }

    /// Linear fusion with score normalization
    /// Score = α × normalized_bm25 + (1-α) × normalized_semantic
    fn linear_fusion(
        &self,
        lexical_results: Vec<RankedResult>,
        semantic_results: Vec<RankedResult>,
        limit: usize,
    ) -> Vec<RankedResult> {
        // Normalize scores to [0, 1]
        let normalized_lexical = Self::normalize_scores(lexical_results);
        let normalized_semantic = Self::normalize_scores(semantic_results);

        let mut scores: HashMap<String, (f32, Option<RankedResult>)> = HashMap::new();

        // Process lexical
        for result in normalized_lexical {
            let key = result.file_path.clone();
            let weighted = result.score * self.config.alpha;

            scores
                .entry(key)
                .and_modify(|(s, r)| {
                    *s += weighted;
                    if r.is_none() {
                        *r = Some(result.clone());
                    }
                })
                .or_insert((weighted, Some(result)));
        }

        // Process semantic
        for result in normalized_semantic {
            let key = result.file_path.clone();
            let weighted = result.score * (1.0 - self.config.alpha);

            scores
                .entry(key)
                .and_modify(|(s, r)| {
                    *s += weighted;
                    if r.is_none() {
                        *r = Some(result.clone());
                    }
                })
                .or_insert((weighted, Some(result)));
        }

        // Sort and return
        let mut results: Vec<_> = scores
            .into_iter()
            .filter_map(|(_, (score, result))| {
                result.map(|mut r| {
                    r.score = score;
                    r.source = SearchSource::Hybrid;
                    r
                })
            })
            .collect();

        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
        results.truncate(limit);
        results
    }

    /// Min-max normalization to [0, 1]
    fn normalize_scores(results: Vec<RankedResult>) -> Vec<RankedResult> {
        if results.is_empty() {
            return results;
        }

        let min = results
            .iter()
            .map(|r| r.score)
            .fold(f32::INFINITY, f32::min);
        let max = results
            .iter()
            .map(|r| r.score)
            .fold(f32::NEG_INFINITY, f32::max);
        let range = max - min;

        if range <= 0.0 {
            // All scores are the same, normalize to 1.0
            return results
                .into_iter()
                .map(|mut r| {
                    r.score = 1.0;
                    r
                })
                .collect();
        }

        results
            .into_iter()
            .map(|mut r| {
                r.score = (r.score - min) / range;
                r
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_result(path: &str, score: f32, source: SearchSource) -> RankedResult {
        RankedResult {
            file_path: path.to_string(),
            chunk_id: None,
            score,
            source,
            start_line: 1,
            end_line: 10,
            content_preview: "test".to_string(),
            name: None,
        }
    }

    #[test]
    fn test_rrf_fusion() {
        let ranker = HybridRanker::new(HybridConfig::default());

        let lexical = vec![
            make_result("a.rs", 10.0, SearchSource::Lexical),
            make_result("b.rs", 8.0, SearchSource::Lexical),
        ];

        let semantic = vec![
            make_result("b.rs", 0.9, SearchSource::Semantic),
            make_result("c.rs", 0.8, SearchSource::Semantic),
        ];

        let results = ranker.fuse(lexical, semantic, 10);

        // b.rs should be first since it appears in both
        assert!(results[0].file_path == "b.rs" || results[1].file_path == "b.rs");
    }
}
