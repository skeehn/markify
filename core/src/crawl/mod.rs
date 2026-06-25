//! Crawl module: URL frontier management, deduplication, politeness.
//!
//! Phase 1: Basic single-URL scraping (this module is a stub).
//! Phase 2: Full distributed crawling with URL frontier, bloom filter dedup, robots.txt,
//!            change detection, checkpointing, and adaptive scheduling.

pub mod checkpoint;
pub mod dedup;
pub mod engine;
pub mod frontier;
pub mod politeness;
pub mod sitemap;

pub use engine::{
    extract_domain, matches_patterns, ChangeDetectionResult, ContentFingerprint, CrawlBloomFilter,
    CrawlCheckpoint, CrawlEngineConfig, CrawlJob as EngineCrawlJob, CrawlJobState, DomainState,
    FrontierStats, FrontierUrl, UrlFrontier, UrlPriority,
};

use serde::{Deserialize, Serialize};

/// Crawl request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrawlRequest {
    /// Seed URL
    pub url: String,
    /// Maximum pages to crawl
    pub max_pages: Option<usize>,
    /// Only crawl within the same domain
    #[serde(default = "default_true")]
    pub same_domain: bool,
    /// URL patterns to include (regex)
    pub include_patterns: Option<Vec<String>>,
    /// URL patterns to exclude (regex)
    pub exclude_patterns: Option<Vec<String>>,
    /// Output format
    pub format: Option<String>,
    /// Respect robots.txt
    #[serde(default = "default_true")]
    pub respect_robots: bool,
    /// Delay between requests (ms)
    pub delay_ms: Option<u64>,
}

fn default_true() -> bool {
    true
}

/// Crawl job status (legacy API compat)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrawlJob {
    pub id: String,
    pub status: CrawlStatus,
    pub seed_url: String,
    pub pages_crawled: usize,
    pub pages_queued: usize,
    pub pages_failed: usize,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CrawlStatus {
    Queued,
    Running,
    Completed,
    Failed,
    Cancelled,
}

/// Crawl result for a single page
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrawlPageResult {
    pub url: String,
    pub status_code: u16,
    pub title: Option<String>,
    pub markdown: Option<String>,
    pub discovered_links: Vec<String>,
    pub error: Option<String>,
}
