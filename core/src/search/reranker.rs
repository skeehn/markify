//! Cross-Encoder Re-Ranking using ONNX MiniLM.
//!
//! Re-ranks search results using a cross-encoder model for higher accuracy.
//! Uses ONNX Runtime for fast, local inference.
//!
//! Pipeline:
//! 1. Take top 50-100 candidates from BM25/HNSW
//! 2. Score each (query, document) pair with cross-encoder
//! 3. Re-sort by cross-encoder score
//! 4. Return top-K results

use serde::{Deserialize, Serialize};

/// Cross-encoder re-ranking configuration
#[derive(Debug, Clone)]
pub struct CrossEncoderConfig {
    /// Model to use (ONNX path or built-in)
    pub model_path: Option<String>,
    /// Maximum candidates to re-rank
    pub max_candidates: usize,
    /// Maximum results to return after re-ranking
    pub top_k: usize,
    /// Timeout for re-ranking (ms)
    pub timeout_ms: u64,
}

impl Default for CrossEncoderConfig {
    fn default() -> Self {
        Self {
            model_path: None,
            max_candidates: 50,
            top_k: 10,
            timeout_ms: 200, // 200ms budget
        }
    }
}

/// Re-ranked search result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReRankedResult {
    pub block_id: String,
    pub url: String,
    pub title: String,
    pub text_snippet: String,
    pub block_type: String,
    /// Original BM25 score
    pub bm25_score: Option<f64>,
    /// Original dense similarity
    pub dense_similarity: Option<f64>,
    /// Cross-encoder score (primary ranking signal after re-ranking)
    pub cross_encoder_score: f64,
    /// Final hybrid score
    pub final_score: f64,
    /// Rank after re-ranking
    pub rank: usize,
}

/// Cross-encoder re-ranker
pub struct CrossEncoderReranker {
    config: CrossEncoderConfig,
    /// ONNX session (loaded lazily)
    #[cfg(feature = "cross-encoder")]
    session: Option<ort::Session>,
}

impl CrossEncoderReranker {
    pub fn new(config: CrossEncoderConfig) -> Self {
        Self {
            config,
            #[cfg(feature = "cross-encoder")]
            session: None,
        }
    }

    /// Load the ONNX model
    #[cfg(feature = "cross-encoder")]
    pub fn load_model(&mut self, model_path: &str) -> anyhow::Result<()> {
        let session = ort::Session::builder()?
            .with_intra_threads(4)?
            .with_inter_threads(2)?
            .commit_from_file(model_path)?;
        self.session = Some(session);
        debug!(model_path, "Cross-encoder model loaded");
        Ok(())
    }

    /// Re-rank search results using cross-encoder scoring
    pub fn rerank(&self, query: &str, candidates: Vec<CandidateDocument>) -> Vec<ReRankedResult> {
        if candidates.is_empty() {
            return Vec::new();
        }

        // Limit candidates
        let candidates: Vec<_> = candidates
            .into_iter()
            .take(self.config.max_candidates)
            .collect();

        // Score each candidate
        let mut scored: Vec<(CandidateDocument, f64)> = candidates
            .into_iter()
            .map(|doc| {
                let score = self.score_pair(query, &doc.text);
                (doc, score)
            })
            .collect();

        // Sort by cross-encoder score descending
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Take top-K and build results
        scored
            .into_iter()
            .take(self.config.top_k)
            .enumerate()
            .map(|(rank, (doc, ce_score))| ReRankedResult {
                block_id: doc.block_id,
                url: doc.url,
                title: doc.title,
                text_snippet: doc.text_snippet,
                block_type: doc.block_type,
                bm25_score: doc.bm25_score,
                dense_similarity: doc.dense_similarity,
                cross_encoder_score: ce_score,
                final_score: self.combine_scores(
                    doc.bm25_score,
                    doc.dense_similarity,
                    ce_score,
                    rank,
                ),
                rank: rank + 1,
            })
            .collect()
    }

    /// Score a (query, document) pair
    fn score_pair(&self, query: &str, document: &str) -> f64 {
        #[cfg(feature = "cross-encoder")]
        {
            if let Some(ref session) = self.session {
                return self.run_onnx_inference(session, query, document);
            }
        }

        // Fallback: use heuristic scoring based on overlap
        self.heuristic_score(query, document)
    }

    /// Run ONNX inference for cross-encoder scoring
    #[cfg(feature = "cross-encoder")]
    fn run_onnx_inference(&self, session: &ort::Session, _query: &str, _document: &str) -> f64 {
        // ONNX inference implementation
        // In production, this would tokenize (query, document) pair,
        // run through MiniLM cross-encoder, and extract the relevance score
        0.5 // Placeholder
    }

    /// Heuristic fallback scoring (when ONNX model not available)
    fn heuristic_score(&self, query: &str, document: &str) -> f64 {
        let query_terms: std::collections::HashSet<String> = query
            .to_lowercase()
            .split_whitespace()
            .filter(|w| w.len() > 2)
            .map(|s| s.to_string())
            .collect();

        if query_terms.is_empty() {
            return 0.0;
        }

        let doc_lower = document.to_lowercase();
        let mut matched = 0;

        for term in &query_terms {
            if doc_lower.contains(term) {
                matched += 1;
            }
        }

        // Jaccard-like score
        matched as f64 / query_terms.len() as f64
    }

    /// Combine original scores with cross-encoder score
    fn combine_scores(
        &self,
        bm25_score: Option<f64>,
        dense_sim: Option<f64>,
        ce_score: f64,
        rank: usize,
    ) -> f64 {
        // Weighted combination: 60% cross-encoder, 20% BM25, 20% dense
        let bm25_norm = bm25_score.unwrap_or(0.0) / 20.0; // Normalize BM25
        let dense_norm = dense_sim.unwrap_or(0.0); // Already 0-1

        0.6 * ce_score + 0.2 * bm25_norm.min(1.0) + 0.2 * dense_norm - 0.01 * rank as f64
        // Small rank penalty
    }
}

/// Candidate document for re-ranking
#[derive(Debug, Clone)]
pub struct CandidateDocument {
    pub block_id: String,
    pub url: String,
    pub title: String,
    pub text_snippet: String,
    pub text: String, // Full text for scoring
    pub block_type: String,
    pub bm25_score: Option<f64>,
    pub dense_similarity: Option<f64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_heuristic_scoring() {
        let reranker = CrossEncoderReranker::new(CrossEncoderConfig::default());

        let score = reranker.heuristic_score(
            "web scraping tool",
            "web scraping is a technique for extracting data",
        );
        assert!(score > 0.5); // Should match "web" and "scraping"

        let score_low = reranker.heuristic_score("quantum computing", "baking cookies recipe");
        assert!(score_low < 0.2); // No overlap
    }

    #[test]
    fn test_combine_scores() {
        let reranker = CrossEncoderReranker::new(CrossEncoderConfig::default());

        let score = reranker.combine_scores(Some(15.0), Some(0.8), 0.9, 0);
        // 0.6 * 0.9 + 0.2 * 0.75 + 0.2 * 0.8 - 0
        assert!(score > 0.7);

        let score_low = reranker.combine_scores(None, None, 0.1, 5);
        // 0.6 * 0.1 + 0 + 0 - 0.05 = 0.01
        assert!(score_low < 0.1);
    }

    #[test]
    fn test_rerank_empty() {
        let reranker = CrossEncoderReranker::new(CrossEncoderConfig::default());
        let results = reranker.rerank("test", vec![]);
        assert!(results.is_empty());
    }
}
