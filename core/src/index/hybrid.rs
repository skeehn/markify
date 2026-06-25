//! Hybrid Search with Reciprocal Rank Fusion (RRF).
//!
//! Combines BM25 (sparse) and HNSW (dense) search results using RRF,
//! a parameter-light fusion method proven in production systems.
//!
//! RRF score = sum(1 / (k + rank_i)) for each retriever i
//! where k is a constant (default 60) and rank_i is the rank in retriever i.

use std::collections::HashMap;

use crate::index::dense::{DenseIndex, DenseSearchResult as DenseResult};
use crate::index::sparse::{SparseIndex, SparseSearchResult as Bm25Result};

/// Hybrid search result with scores from both retrievers
#[derive(Debug, Clone, serde::Serialize)]
pub struct HybridSearchResult {
    pub block_id: String,
    pub url: String,
    pub title: String,
    pub text_snippet: String,
    pub block_type: String,
    /// RRF combined score
    pub hybrid_score: f64,
    /// BM25 score (if found via BM25)
    pub bm25_score: Option<f64>,
    pub bm25_rank: Option<usize>,
    /// Dense cosine similarity (if found via dense)
    pub dense_similarity: Option<f64>,
    pub dense_rank: Option<usize>,
}

/// RRF configuration
#[derive(Debug, Clone)]
pub struct RrfConfig {
    /// Constant k in RRF formula (default 60)
    /// k=60 is the widely-used default from the original paper
    pub k: f64,
    /// Weight for BM25 results (default 1.0)
    pub bm25_weight: f64,
    /// Weight for dense results (default 1.0)
    pub dense_weight: f64,
    /// Maximum results to return
    pub limit: usize,
}

impl Default for RrfConfig {
    fn default() -> Self {
        Self {
            k: 60.0,
            bm25_weight: 1.0,
            dense_weight: 1.0,
            limit: 20,
        }
    }
}

/// Perform Reciprocal Rank Fusion on BM25 and dense search results.
///
/// # Arguments
/// * `bm25_results` — results from BM25 (Tantivy) search
/// * `dense_results` — results from dense (HNSW/cosine) search
/// * `config` — RRF configuration
///
/// # Returns
/// Deduplicated, re-ranked results sorted by hybrid RRF score
pub fn reciprocal_rank_fusion(
    bm25_results: Vec<Bm25Result>,
    dense_results: Vec<DenseResult>,
    config: &RrfConfig,
) -> Vec<HybridSearchResult> {
    // Map block_id → hybrid result
    let mut results_map: HashMap<String, HybridSearchResult> = HashMap::new();

    // Process BM25 results
    for (rank, result) in bm25_results.iter().enumerate() {
        let rrf_score = config.bm25_weight / (config.k + rank as f64);

        let entry = results_map
            .entry(result.block_id.clone())
            .or_insert_with(|| HybridSearchResult {
                block_id: result.block_id.clone(),
                url: result.url.clone(),
                title: result.title.clone(),
                text_snippet: result.text_snippet.clone(),
                block_type: result.block_type.clone(),
                hybrid_score: 0.0,
                bm25_score: None,
                bm25_rank: None,
                dense_similarity: None,
                dense_rank: None,
            });

        entry.hybrid_score += rrf_score;
        entry.bm25_score = Some(result.score);
        entry.bm25_rank = Some(rank + 1);
    }

    // Process dense results
    for (rank, result) in dense_results.iter().enumerate() {
        let rrf_score = config.dense_weight / (config.k + rank as f64);

        let entry = results_map
            .entry(result.block_id.clone())
            .or_insert_with(|| HybridSearchResult {
                block_id: result.block_id.clone(),
                url: result.url.clone(),
                title: result.title.clone(),
                text_snippet: result.text_snippet.clone(),
                block_type: result.block_type.clone(),
                hybrid_score: 0.0,
                bm25_score: None,
                bm25_rank: None,
                dense_similarity: None,
                dense_rank: None,
            });

        entry.hybrid_score += rrf_score;
        entry.dense_similarity = Some(result.similarity);
        entry.dense_rank = Some(rank + 1);
    }

    // Sort by hybrid score descending
    let mut results: Vec<HybridSearchResult> = results_map.into_values().collect();
    results.sort_by(|a, b| {
        b.hybrid_score
            .partial_cmp(&a.hybrid_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Truncate to limit
    results.truncate(config.limit);
    results
}

/// Hybrid search executor — runs both BM25 and dense search, then fuses.
pub struct HybridSearcher {
    sparse_index: SparseIndex,
    dense_index: DenseIndex,
}

impl HybridSearcher {
    /// Create a new hybrid searcher
    pub fn new(sparse_index: SparseIndex, dense_index: DenseIndex) -> Self {
        Self {
            sparse_index,
            dense_index,
        }
    }

    /// Search using both BM25 and dense, then fuse with RRF
    pub fn search(
        &self,
        query: &str,
        config: &RrfConfig,
    ) -> anyhow::Result<Vec<HybridSearchResult>> {
        // Run both searches in parallel conceptually (sequentially here)
        let bm25_results = self.sparse_index.search(query, config.limit)?;
        let dense_results = self.dense_index.search(query, config.limit);

        // Fuse with RRF
        let fused = reciprocal_rank_fusion(bm25_results, dense_results, config);

        Ok(fused)
    }

    /// Search using BM25 only
    pub fn search_bm25_only(
        &self,
        query: &str,
        limit: usize,
    ) -> anyhow::Result<Vec<HybridSearchResult>> {
        let bm25_results = self.sparse_index.search(query, limit)?;

        Ok(bm25_results
            .into_iter()
            .enumerate()
            .map(|(rank, r)| HybridSearchResult {
                block_id: r.block_id,
                url: r.url,
                title: r.title,
                text_snippet: r.text_snippet,
                block_type: r.block_type,
                hybrid_score: r.score,
                bm25_score: Some(r.score),
                bm25_rank: Some(rank + 1),
                dense_similarity: None,
                dense_rank: None,
            })
            .collect())
    }

    /// Search using dense only
    pub fn search_dense_only(&self, query: &str, limit: usize) -> Vec<HybridSearchResult> {
        let dense_results = self.dense_index.search(query, limit);

        dense_results
            .into_iter()
            .enumerate()
            .map(|(rank, r)| HybridSearchResult {
                block_id: r.block_id,
                url: r.url,
                title: r.title,
                text_snippet: r.text_snippet,
                block_type: r.block_type,
                hybrid_score: r.similarity,
                bm25_score: None,
                bm25_rank: None,
                dense_similarity: Some(r.similarity),
                dense_rank: Some(rank + 1),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_bm25_result(id: &str, score: f64) -> Bm25Result {
        Bm25Result {
            block_id: id.to_string(),
            url: "https://example.com".to_string(),
            title: format!("Title {}", id),
            text_snippet: format!("Text {}", id),
            score,
            block_type: "article".to_string(),
            source_url: "https://example.com".to_string(),
        }
    }

    fn make_dense_result(id: &str, sim: f64) -> DenseResult {
        DenseResult {
            block_id: id.to_string(),
            url: "https://example.com".to_string(),
            title: format!("Title {}", id),
            text_snippet: format!("Text {}", id),
            similarity: sim,
            block_type: "article".to_string(),
        }
    }

    #[test]
    fn test_rrf_fusion_no_overlap() {
        let bm25 = vec![make_bm25_result("a", 10.0), make_bm25_result("b", 8.0)];
        let dense = vec![make_dense_result("c", 0.9), make_dense_result("d", 0.8)];

        let config = RrfConfig::default();
        let results = reciprocal_rank_fusion(bm25, dense, &config);

        assert_eq!(results.len(), 4);
        // "a" and "c" should be top (both rank 1, same RRF score)
        assert!(results[0].hybrid_score > 0.0);
    }

    #[test]
    fn test_rrf_fusion_with_overlap() {
        let bm25 = vec![make_bm25_result("a", 10.0), make_bm25_result("b", 8.0)];
        let dense = vec![
            make_dense_result("a", 0.9), // "a" in both
            make_dense_result("c", 0.8),
        ];

        let config = RrfConfig::default();
        let results = reciprocal_rank_fusion(bm25, dense, &config);

        assert_eq!(results.len(), 3);
        // "a" should be top (appears in both lists)
        assert_eq!(results[0].block_id, "a");
        assert!(results[0].bm25_score.is_some());
        assert!(results[0].dense_similarity.is_some());
    }

    #[test]
    fn test_rrf_ranking_order() {
        let bm25 = vec![make_bm25_result("top", 15.0), make_bm25_result("mid", 10.0)];
        let dense = vec![
            make_dense_result("top", 0.95),
            make_dense_result("mid", 0.85),
        ];

        let config = RrfConfig::default();
        let results = reciprocal_rank_fusion(bm25, dense, &config);

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].block_id, "top");
        assert_eq!(results[1].block_id, "mid");
        assert!(results[0].hybrid_score > results[1].hybrid_score);
    }
}
