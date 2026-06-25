//! # Markify Core
//!
//! The Apache-2.0-licensed web data extraction engine for AI agents.
//!
//! ## Quick Start
//!
//! ```no_run
//! use markify_core::{Markify, ScrapeRequest, OutputFormat};
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let client = Markify::default();
//!     let (result, _meta) = client.scrape(ScrapeRequest {
//!         url: "https://example.com".to_string(),
//!         formats: vec![OutputFormat::Markdown, OutputFormat::Json],
//!         ..Default::default()
//!     }).await?;
//!     println!("{}", result.markdown.unwrap());
//!     Ok(())
//! }
//! ```

pub mod cache;
pub mod cilow;
pub mod crawl;
pub mod extract;
pub mod fetch;
pub mod index;
pub mod neural_search;
pub mod renderless;
pub mod scrape;
pub mod search;
pub mod structured_api;
pub mod telemetry;
pub mod transform;
pub mod vsb_graph;

pub use cache::CacheConfig;
pub use cilow::CilowClient;
pub use extract::{ExtractedContent, ExtractionMode, ImageInfo, LinkInfo, Metadata};
pub use fetch::FetchConfig;
pub use index::dense::{DenseIndex, DenseSearchResult, DenseVector};
pub use index::hybrid::{HybridSearchResult, HybridSearcher, RrfConfig};
pub use index::sparse::{SparseIndex, SparseSearchResult};
pub use neural_search::ExaClient;
pub use scrape::{Markify, ScrapeMeta, ScrapeRequest, ScrapeResult};
pub use search::{SearchClient, SearchConfig, SearchResult};
pub use structured_api::spec::{ApiSpec, Endpoint, ExecutionResult};
pub use structured_api::{execute_api_spec, generate_api_spec};
pub use telemetry::Telemetry;
pub use transform::OutputFormat;
pub use vsb_graph::{classify_block, segment_page, VSBGraph};

pub use search::query_understanding::{
    extract_entities, rewrite_query, understand_query, IntentClassifier, QueryIntent,
    QueryRewriteResult, QueryUnderstandingResult,
};
pub use search::reranker::{
    CandidateDocument, CrossEncoderConfig, CrossEncoderReranker, ReRankedResult,
};

pub use telemetry::otel::{
    MetricsSummary, OtelExporter, OtelObservability, TraceContext, TraceMiddleware,
};

pub use structured_api::extraction::{
    detect_pagination, execute_program, generate_program, verify_program, ExtractionMethod,
    ExtractionProgram, ExtractionSchema, ExtractionStep, FieldType, FieldVerification,
    LLMExtractionRequest, LLMGeneratedSchema, PaginationType, PostProcess, SchemaField,
    VerificationResult,
};

/// Library version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
