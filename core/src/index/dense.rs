//! Dense Vector Index using HNSW (usearch) for approximate nearest neighbor search.
//!
//! Phase 1: TF-IDF cosine similarity (no ML dependency) — ✅
//! Phase 2: usearch HNSW + fastembed BGE-small — ✅
//! Phase 3: Quantization (PQ/SQ) for memory efficiency — TODO

use std::collections::HashMap;
use std::sync::Mutex;

use tracing::debug;
use usearch::ffi::Matches;
use usearch::{Index, IndexOptions, MetricKind, ScalarKind};

/// Dense vector (fixed dimension)
#[derive(Debug, Clone)]
pub struct DenseVector {
    pub data: Vec<f32>,
    pub dim: usize,
}

impl DenseVector {
    pub fn new(data: Vec<f32>) -> Self {
        let dim = data.len();
        Self { data, dim }
    }

    /// Cosine similarity with another vector
    pub fn cosine_similarity(&self, other: &Self) -> f32 {
        if self.dim != other.dim || self.dim == 0 {
            return 0.0;
        }

        let mut dot = 0.0f32;
        let mut norm_a = 0.0f32;
        let mut norm_b = 0.0f32;

        for i in 0..self.dim {
            dot += self.data[i] * other.data[i];
            norm_a += self.data[i] * self.data[i];
            norm_b += other.data[i] * other.data[i];
        }

        let denom = norm_a.sqrt() * norm_b.sqrt();
        if denom == 0.0 {
            0.0
        } else {
            dot / denom
        }
    }
}

/// Embedding backend: TF-IDF or ML (fastembed)
#[cfg(feature = "ml-embeddings")]
mod embedder {
    use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
    use once_cell::sync::OnceCell;

    static MODEL: OnceCell<TextEmbedding> = OnceCell::new();

    pub fn get_model() -> &'static TextEmbedding {
        MODEL.get_or_init(|| {
            TextEmbedding::try_new(
                InitOptions::new(EmbeddingModel::BGESmallENV15).with_show_download_progress(true),
            )
            .expect("Failed to load BGE-Small embedding model")
        })
    }

    pub fn embed_text(text: &str) -> Vec<f32> {
        let model = get_model();
        let embeddings = model
            .embed(vec![text.to_string()], None)
            .expect("Embedding failed");
        embeddings.into_iter().next().unwrap_or_default()
    }

    pub fn embedding_dim() -> usize {
        384 // BGE-Small-EN-v1.5 dimension
    }
}

#[cfg(not(feature = "ml-embeddings"))]
mod embedder {
    pub fn embed_text(_text: &str) -> Vec<f32> {
        Vec::new()
    }

    pub fn embedding_dim() -> usize {
        384
    }
}

/// TF-IDF based text embedding
fn text_to_tfidf_vector(text: &str, vocab: &HashMap<String, usize>, dim: usize) -> DenseVector {
    let mut vec = vec![0.0f32; dim];

    let tokens: Vec<String> = text
        .to_lowercase()
        .split_whitespace()
        .map(|t: &str| t.chars().filter(|c| c.is_alphanumeric()).collect())
        .filter(|t: &String| t.len() > 2)
        .collect();

    let mut tf: HashMap<String, usize> = HashMap::new();
    for token in &tokens {
        *tf.entry(token.clone()).or_insert(0) += 1;
    }

    let total = tokens.len() as f32;
    for (term, count) in &tf {
        if let Some(&idx) = vocab.get(term) {
            if idx < dim {
                let val = *count as f32 / total;
                vec[idx] = (val + 1.0).ln();
            }
        }
    }

    DenseVector::new(vec)
}

/// Search result from dense index
#[derive(Debug, Clone, serde::Serialize)]
pub struct DenseSearchResult {
    pub block_id: String,
    pub url: String,
    pub title: String,
    pub text_snippet: String,
    pub similarity: f64,
    pub block_type: String,
}

/// Entry stored in the dense index
struct IndexEntry {
    block_id: String,
    url: String,
    title: String,
    text: String,
    block_type: String,
    vector: DenseVector,
    hnsw_key: u64,
}

/// HNSW-backed dense vector index using usearch
pub struct DenseIndex {
    /// HNSW index (usearch)
    hnsw: Option<Index>,
    /// Metadata for matched entries
    entries: Vec<IndexEntry>,
    /// Vocabulary for TF-IDF mode
    vocab: HashMap<String, usize>,
    /// Embedding dimension
    dim: usize,
    /// Whether to use ML embeddings
    use_ml: bool,
    /// Next HNSW key for insertion
    next_hnsw_key: u64,
    /// Mapping from HNSW key → entry index
    key_to_idx: HashMap<u64, usize>,
    /// Lock for thread-safe HNSW access
    _lock: Mutex<()>,
}

impl DenseIndex {
    /// Create a new HNSW-backed dense index
    pub fn new(dim: usize) -> Self {
        Self {
            hnsw: None,
            entries: Vec::new(),
            vocab: HashMap::new(),
            dim,
            use_ml: cfg!(feature = "ml-embeddings"),
            next_hnsw_key: 0,
            key_to_idx: HashMap::new(),
            _lock: Mutex::new(()),
        }
    }

    /// Initialize the HNSW index. Call this before searching.
    pub fn init_hnsw(&mut self) -> anyhow::Result<()> {
        if self.hnsw.is_some() {
            return Ok(());
        }

        let mut options = IndexOptions::default();
        options.dimensions = self.dim;
        options.metric = MetricKind::Cos;
        options.quantization = ScalarKind::F32;

        let index = Index::new(&options)?;
        index.reserve(10000)?; // Reserve for 10K entries

        self.hnsw = Some(index);
        debug!(dim = self.dim, "HNSW index initialized");
        Ok(())
    }

    /// Build vocabulary from all entries (for TF-IDF mode) and add to HNSW.
    pub fn build_vocab(&mut self) -> anyhow::Result<()> {
        if self.entries.is_empty() {
            return Ok(());
        }

        // Don't rebuild if already built
        if self.hnsw.is_some() && self.hnsw.as_ref().map_or(0, |h| h.size()) == self.entries.len() {
            return Ok(());
        }

        self.init_hnsw()?;

        if self.use_ml {
            // ML mode: compute embeddings via fastembed
            for entry in &mut self.entries {
                if entry.vector.data.is_empty()
                    || entry.vector.data.len() != embedder::embedding_dim()
                {
                    entry.vector = DenseVector::new(embedder::embed_text(&entry.text));
                }
            }

            // Add to HNSW
            if let Some(ref hnsw) = self.hnsw {
                for entry in &self.entries {
                    if entry.vector.data.len() == self.dim {
                        let key = self.next_hnsw_key;
                        hnsw.add(key, &entry.vector.data)?;
                        self.key_to_idx.insert(
                            key,
                            self.entries
                                .iter()
                                .position(|e| e.block_id == entry.block_id)
                                .unwrap(),
                        );
                        self.next_hnsw_key += 1;
                    }
                }
            }

            debug!(
                entries = self.entries.len(),
                dim = embedder::embedding_dim(),
                "ML embeddings built and added to HNSW"
            );
            return Ok(());
        }

        // TF-IDF mode: build vocabulary and vectors
        let mut term_freq = HashMap::new();
        let mut doc_freq = HashMap::new();

        for entry in &self.entries {
            let tokens: Vec<String> = entry
                .text
                .to_lowercase()
                .split_whitespace()
                .map(|t: &str| t.chars().filter(|c| c.is_alphanumeric()).collect())
                .filter(|t: &String| t.len() > 2)
                .collect();

            for token in tokens {
                *term_freq.entry(token.clone()).or_insert(0usize) += 1;
                doc_freq
                    .entry(token)
                    .and_modify(|d: &mut usize| *d += 1)
                    .or_insert(1usize);
            }
        }

        // Keep top-K terms by TF-IDF score
        let n_docs = self.entries.len() as f32;
        let mut scored_terms: Vec<(String, f32)> = term_freq
            .iter()
            .map(|(term, tf)| {
                let df = doc_freq.get(term).copied().unwrap_or(1) as f32;
                let idf = (n_docs / df.max(1.0)).log10() + 1.0;
                (term.clone(), *tf as f32 * idf)
            })
            .collect();

        scored_terms.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let top_n = scored_terms.len().min(self.dim);
        for (i, (term, _)) in scored_terms.into_iter().take(top_n).enumerate() {
            self.vocab.insert(term, i);
        }

        // Build vectors for all entries
        for entry in &mut self.entries {
            entry.vector = text_to_tfidf_vector(&entry.text, &self.vocab, self.dim);
        }

        // Add to HNSW
        if let Some(ref hnsw) = self.hnsw {
            for entry in &self.entries {
                if entry.vector.data.len() == self.dim {
                    let key = self.next_hnsw_key;
                    hnsw.add(key, &entry.vector.data)?;
                    self.key_to_idx.insert(
                        key,
                        self.entries
                            .iter()
                            .position(|e| e.block_id == entry.block_id)
                            .unwrap(),
                    );
                    self.next_hnsw_key += 1;
                }
            }
        }

        debug!(
            vocab_size = self.vocab.len(),
            entries = self.entries.len(),
            dim = self.dim,
            "TF-IDF vocabulary built and added to HNSW"
        );
        Ok(())
    }

    /// Add an entry (vector will be built on build_vocab call)
    pub fn add_entry(
        &mut self,
        block_id: &str,
        url: &str,
        title: &str,
        text: &str,
        block_type: &str,
    ) {
        let vector = if self.use_ml && !text.is_empty() {
            DenseVector::new(embedder::embed_text(text))
        } else {
            DenseVector::new(vec![0.0; self.dim])
        };

        self.entries.push(IndexEntry {
            block_id: block_id.to_string(),
            url: url.to_string(),
            title: title.to_string(),
            text: text.to_string(),
            block_type: block_type.to_string(),
            vector,
            hnsw_key: self.next_hnsw_key,
        });
        self.next_hnsw_key += 1;
    }

    /// Search by query text using exact cosine similarity (accurate, brute-force)
    pub fn search(&self, query: &str, limit: usize) -> Vec<DenseSearchResult> {
        if self.entries.is_empty() {
            return Vec::new();
        }

        let query_vec = if self.use_ml {
            DenseVector::new(embedder::embed_text(query))
        } else {
            text_to_tfidf_vector(query, &self.vocab, self.dim)
        };

        if query_vec.data.len() != self.dim {
            return Vec::new();
        }

        // Exact cosine similarity search
        let mut scored: Vec<_> = self
            .entries
            .iter()
            .map(|entry| {
                let sim = entry.vector.cosine_similarity(&query_vec);
                (entry, sim)
            })
            .filter(|(_, sim)| *sim > 0.0)
            .collect();

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        scored
            .into_iter()
            .take(limit)
            .map(|(entry, sim)| {
                let snippet = if entry.text.len() > 200 {
                    format!("{}...", &entry.text[..200])
                } else {
                    entry.text.clone()
                };

                DenseSearchResult {
                    block_id: entry.block_id.clone(),
                    url: entry.url.clone(),
                    title: entry.title.clone(),
                    text_snippet: snippet,
                    similarity: sim as f64,
                    block_type: entry.block_type.clone(),
                }
            })
            .collect()
    }

    /// Search using HNSW approximate nearest neighbor (fast, approximate)
    pub fn search_hnsw(&self, query: &str, limit: usize) -> Vec<DenseSearchResult> {
        if self.entries.is_empty() || self.hnsw.is_none() {
            return Vec::new();
        }

        let query_vec = if self.use_ml {
            DenseVector::new(embedder::embed_text(query))
        } else {
            text_to_tfidf_vector(query, &self.vocab, self.dim)
        };

        if query_vec.data.len() != self.dim {
            return Vec::new();
        }

        let Some(ref hnsw) = self.hnsw else {
            return Vec::new();
        };

        let matches: Matches = match hnsw.search(&query_vec.data, limit) {
            Ok(m) => m,
            Err(_) => return Vec::new(),
        };

        // Build key → entry mapping
        let key_to_entry: HashMap<u64, &IndexEntry> =
            self.entries.iter().map(|e| (e.hnsw_key, e)).collect();

        matches
            .keys
            .iter()
            .zip(matches.distances.iter())
            .filter_map(|(key, distance)| {
                let entry = key_to_entry.get(key)?;
                // Cosine distance → similarity (distance = 1 - cosine_sim for Cos metric)
                let similarity = 1.0 - distance;
                let snippet = if entry.text.len() > 200 {
                    format!("{}...", &entry.text[..200])
                } else {
                    entry.text.clone()
                };

                Some(DenseSearchResult {
                    block_id: entry.block_id.clone(),
                    url: entry.url.clone(),
                    title: entry.title.clone(),
                    text_snippet: snippet,
                    similarity: similarity as f64,
                    block_type: entry.block_type.clone(),
                })
            })
            .collect()
    }

    /// Get total entry count
    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }
}

/// Neural search configuration
#[derive(Debug, Clone)]
pub struct NeuralSearchConfig {
    /// Embedding dimension (384 for BGE-small, 768 for BGE-base)
    pub dim: usize,
    /// Whether to use ML embeddings (requires fastembed feature)
    pub use_ml: bool,
}

impl Default for NeuralSearchConfig {
    fn default() -> Self {
        Self {
            dim: 384,
            use_ml: cfg!(feature = "ml-embeddings"),
        }
    }
}
