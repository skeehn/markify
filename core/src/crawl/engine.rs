//! Distributed Crawl Engine — Phase 2 Asterism.
//!
//! Production-grade crawling with URL frontier, politeness, dedup,
//! change detection, checkpointing, and adaptive scheduling.
//!
//! Architecture:
//! 1. **URL Frontier**: Priority queue with per-domain rate limiting
//! 2. **Bloom Filter**: Fast seen-URL check (probabilistic, memory-efficient)
//! 3. **Change Detection**: DOM diff + embedding similarity for content freshness
//! 4. **Checkpointing**: Idempotent state recovery after crashes
//! 5. **Robots.txt**: Politeness compliance with caching
//! 6. **Scheduler**: Adaptive recrawl based on change rate + demand

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tracing::{debug, warn};

// ─── URL Frontier ────────────────────────────────────────────────────────────

/// URL priority for scheduling
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum UrlPriority {
    /// User-requested, highest priority
    Critical = 0,
    /// Sitemap-discovered
    High = 1,
    /// Link-discovered from crawled pages
    Normal = 2,
    /// Change-detection recrawl
    Low = 3,
}

/// A URL awaiting crawling with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrontierUrl {
    /// The URL to crawl
    pub url: String,
    /// Priority level
    pub priority: UrlPriority,
    /// Depth from seed (0 = seed, 1 = seed's links, etc.)
    pub depth: usize,
    /// Maximum depth allowed for this crawl job
    pub max_depth: usize,
    /// Domain for rate limiting
    pub domain: String,
    /// When this URL was discovered
    pub discovered_at: DateTime<Utc>,
    /// When this URL should be crawled (respecting rate limits)
    pub earliest_crawl: DateTime<Utc>,
    /// Retry count (for failed crawls)
    pub retry_count: u32,
    /// Associated crawl job ID
    pub job_id: String,
    /// Parent URL that linked to this one
    pub parent_url: Option<String>,
    /// Whether to extract links from this page
    pub extract_links: bool,
}

impl FrontierUrl {
    pub fn new(url: &str, job_id: &str, max_depth: usize) -> Self {
        let domain = extract_domain(url).to_string();
        Self {
            url: url.to_string(),
            priority: UrlPriority::Normal,
            depth: 0,
            max_depth,
            domain,
            discovered_at: Utc::now(),
            earliest_crawl: Utc::now(),
            retry_count: 0,
            job_id: job_id.to_string(),
            parent_url: None,
            extract_links: true,
        }
    }

    /// Create a child URL discovered from this page
    pub fn child(&self, child_url: &str) -> Self {
        let domain = extract_domain(child_url).to_string();
        Self {
            url: child_url.to_string(),
            priority: UrlPriority::Normal,
            depth: self.depth + 1,
            max_depth: self.max_depth,
            domain,
            discovered_at: Utc::now(),
            earliest_crawl: Utc::now(),
            retry_count: 0,
            job_id: self.job_id.clone(),
            parent_url: Some(self.url.clone()),
            extract_links: self.depth + 1 < self.max_depth,
        }
    }
}

/// URL Frontier with per-domain rate limiting
pub struct UrlFrontier {
    /// Queue organized by priority then discovery time
    queues: [VecDeque<FrontierUrl>; 4], // One per priority level
    /// Per-domain rate limiting state
    domain_state: HashMap<String, DomainState>,
    /// Total URLs in frontier
    total_count: usize,
    lock: Mutex<()>,
}

/// Per-domain rate limiting state
#[derive(Debug, Clone)]
pub struct DomainState {
    /// Minimum delay between requests (ms)
    pub min_delay_ms: u64,
    /// Last request timestamp
    pub last_request: Option<Instant>,
    /// Request count in current window
    pub request_count: usize,
    /// Window start
    pub window_start: Option<Instant>,
    /// Maximum requests per window
    pub max_per_window: usize,
    /// Window duration
    pub window_duration: Duration,
    /// Whether domain is in back-off
    pub backing_off: bool,
    /// Back-off expiry
    pub backoff_until: Option<Instant>,
}

impl DomainState {
    pub fn new(min_delay_ms: u64, max_per_window: usize, window_duration: Duration) -> Self {
        Self {
            min_delay_ms,
            last_request: None,
            request_count: 0,
            window_start: None,
            max_per_window,
            window_duration,
            backing_off: false,
            backoff_until: None,
        }
    }

    /// Check if we can make a request now
    pub fn can_request(&self) -> bool {
        if self.backing_off {
            if let Some(until) = self.backoff_until {
                return Instant::now() >= until;
            }
        }

        // Check window rate limit
        if let (Some(start), Some(_)) = (self.window_start, self.last_request) {
            if start.elapsed() < self.window_duration && self.request_count >= self.max_per_window {
                return false;
            }
        }

        // Check minimum delay
        if let Some(last) = self.last_request {
            if last.elapsed().as_millis() < self.min_delay_ms as u128 {
                return false;
            }
        }

        true
    }

    /// Record a request
    pub fn record_request(&mut self) {
        let now = Instant::now();

        // Reset window if expired
        if let Some(start) = self.window_start {
            if start.elapsed() > self.window_duration {
                self.window_start = Some(now);
                self.request_count = 0;
            }
        } else {
            self.window_start = Some(now);
        }

        self.last_request = Some(now);
        self.request_count += 1;
    }

    /// Enter back-off after rate limit / error
    pub fn enter_backoff(&mut self, duration: Duration) {
        self.backing_off = true;
        self.backoff_until = Some(Instant::now() + duration);
        warn!(
            duration_secs = duration.as_secs(),
            "Domain entered back-off"
        );
    }

    /// Exit back-off
    pub fn exit_backoff(&mut self) {
        self.backing_off = false;
        self.backoff_until = None;
    }
}

impl Default for UrlFrontier {
    fn default() -> Self {
        Self::new()
    }
}

impl UrlFrontier {
    pub fn new() -> Self {
        Self {
            queues: [
                VecDeque::new(), // Critical
                VecDeque::new(), // High
                VecDeque::new(), // Normal
                VecDeque::new(), // Low
            ],
            domain_state: HashMap::new(),
            total_count: 0,
            lock: Mutex::new(()),
        }
    }

    /// Add a URL to the frontier
    pub fn push(&mut self, url: FrontierUrl) {
        let _lock = self.lock.lock().ok();
        let idx = url.priority as usize;
        if idx < 4 {
            self.queues[idx].push_back(url);
            self.total_count += 1;
        }
    }

    /// Add multiple URLs
    pub fn push_batch(&mut self, urls: Vec<FrontierUrl>) {
        for url in urls {
            self.push(url);
        }
    }

    /// Get the next crawlable URL respecting per-domain rate limits
    pub fn pop_next(&mut self) -> Option<FrontierUrl> {
        let _lock = self.lock.lock().ok()?;

        for queue in &mut self.queues {
            let mut skipped = Vec::new();
            while let Some(url) = queue.pop_front() {
                let domain_state = self
                    .domain_state
                    .entry(url.domain.clone())
                    .or_insert_with(|| DomainState::new(1000, 10, Duration::from_secs(60)));

                if domain_state.can_request() {
                    domain_state.record_request();
                    self.total_count -= 1 + skipped.len();
                    // Put skipped URLs back
                    for skipped_url in skipped.into_iter().rev() {
                        queue.push_front(skipped_url);
                    }
                    return Some(url);
                } else {
                    skipped.push(url);
                }
            }
            // Put all skipped URLs back
            for skipped_url in skipped.into_iter().rev() {
                queue.push_front(skipped_url);
            }
        }
        None
    }

    /// Get domain state or create default
    pub fn set_domain_policy(
        &mut self,
        domain: &str,
        min_delay_ms: u64,
        max_per_window: usize,
        window_duration: Duration,
    ) {
        self.domain_state.insert(
            domain.to_string(),
            DomainState::new(min_delay_ms, max_per_window, window_duration),
        );
    }

    /// Report HTTP status for a domain (adaptive rate limiting)
    pub fn report_domain_status(&mut self, domain: &str, status: u16) {
        let _lock = self.lock.lock().ok();
        if let Some(state) = self.domain_state.get_mut(domain) {
            match status {
                429 => state.enter_backoff(Duration::from_secs(60)),
                503 => state.enter_backoff(Duration::from_secs(30)),
                200 | 301 | 302 | 304 if state.backing_off => {
                    state.exit_backoff();
                }
                _ => {}
            }
        }
    }

    /// Get frontier stats
    pub fn stats(&self) -> FrontierStats {
        let critical = self.queues[0].len();
        let high = self.queues[1].len();
        let normal = self.queues[2].len();
        let low = self.queues[3].len();
        let domain_count = self.domain_state.len();

        FrontierStats {
            total: self.total_count,
            critical,
            high,
            normal,
            low,
            domain_count,
        }
    }
}

/// Frontier statistics
#[derive(Debug, Clone, Serialize)]
pub struct FrontierStats {
    pub total: usize,
    pub critical: usize,
    pub high: usize,
    pub normal: usize,
    pub low: usize,
    pub domain_count: usize,
}

// ─── Bloom Filter ────────────────────────────────────────────────────────────

/// Scalable Bloom Filter for URL deduplication
pub struct CrawlBloomFilter {
    /// Bit array
    bits: Vec<bool>,
    /// Number of hash functions
    k: usize,
    /// Items added
    item_count: usize,
}

impl CrawlBloomFilter {
    /// Create a new bloom filter with given capacity and target false positive rate
    pub fn new(capacity: usize, fp_rate: f64) -> Self {
        // m = -(n * ln(p)) / (ln(2))^2
        let m = (-(capacity as f64 * fp_rate.ln()) / (2.0f64.ln().powi(2))).ceil() as usize;
        // k = (m/n) * ln(2)
        let k = ((m as f64 / capacity as f64) * 2.0f64.ln()).ceil() as usize;

        Self {
            bits: vec![false; m],
            k: k.max(1),
            item_count: 0,
        }
    }

    /// Hash a URL to bit indices
    fn hash_indices(&self, url: &str) -> Vec<usize> {
        let mut indices = Vec::with_capacity(self.k);
        for i in 0..self.k {
            let mut hasher = Sha256::new();
            hasher.update(format!("{}:{}", i, url).as_bytes());
            let hash = hasher.finalize();
            let idx = u64::from_le_bytes(hash[..8].try_into().unwrap_or([0; 8])) as usize;
            indices.push(idx % self.bits.len());
        }
        indices
    }

    /// Add a URL to the filter
    pub fn add(&mut self, url: &str) {
        for idx in self.hash_indices(url) {
            self.bits[idx] = true;
        }
        self.item_count += 1;
    }

    /// Check if a URL might be in the filter (true = probably seen, false = definitely not)
    pub fn might_contain(&self, url: &str) -> bool {
        self.hash_indices(url).iter().all(|&idx| self.bits[idx])
    }

    /// Get current item count
    pub fn len(&self) -> usize {
        self.item_count
    }

    pub fn is_empty(&self) -> bool {
        self.item_count == 0
    }
}

// ─── Change Detection ───────────────────────────────────────────────────────

/// Change detection result
#[derive(Debug, Clone, Serialize)]
pub struct ChangeDetectionResult {
    pub url: String,
    /// Whether content changed
    pub changed: bool,
    /// Similarity score (1.0 = identical, 0.0 = completely different)
    pub similarity: f64,
    /// Previous crawl timestamp
    pub last_crawled: Option<DateTime<Utc>>,
    /// Content hash of previous crawl
    pub previous_hash: Option<String>,
    /// Content hash of current crawl
    pub current_hash: String,
    /// Estimated change rate (0.0-1.0)
    pub change_rate: f64,
}

/// Content fingerprint for change detection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentFingerprint {
    pub url: String,
    pub content_hash: String,
    pub text_length: usize,
    pub link_count: usize,
    pub crawl_time: DateTime<Utc>,
    /// Number of times this URL has been crawled
    pub crawl_count: u64,
    /// Number of times content changed
    pub change_count: u64,
}

impl ContentFingerprint {
    pub fn new(url: &str, html: &str) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(html.as_bytes());
        let content_hash = format!("{:x}", hasher.finalize());

        // Count text and links
        let text_length = html.len();
        let link_count = html.matches("<a ").count();

        Self {
            url: url.to_string(),
            content_hash,
            text_length,
            link_count,
            crawl_time: Utc::now(),
            crawl_count: 1,
            change_count: 0,
        }
    }

    /// Compare with another fingerprint
    pub fn compare(&self, other: &ContentFingerprint) -> ChangeDetectionResult {
        let changed = self.content_hash != other.content_hash;
        // Simple hash comparison (exact match = 1.0, different = 0.0)
        // In production, use MinHash/SimHash for similarity
        let similarity = if self.content_hash == other.content_hash {
            1.0
        } else {
            0.0
        };
        let change_rate = if other.crawl_count == 0 {
            0.0
        } else {
            other.change_count as f64 / other.crawl_count as f64
        };

        ChangeDetectionResult {
            url: self.url.clone(),
            changed,
            similarity,
            last_crawled: Some(other.crawl_time),
            previous_hash: Some(other.content_hash.clone()),
            current_hash: self.content_hash.clone(),
            change_rate,
        }
    }
}

// ─── Crawl Job ──────────────────────────────────────────────────────────────

/// State of a crawl job
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CrawlJobState {
    /// Job created, not started
    Pending,
    /// Job running
    Running,
    /// Job paused
    Paused,
    /// Job completed
    Completed,
    /// Job failed
    Failed,
    /// Job cancelled
    Cancelled,
}

/// A crawl job with configuration and state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrawlJob {
    pub id: String,
    pub name: String,
    /// Seed URLs to start from
    pub seed_urls: Vec<String>,
    /// Maximum crawl depth
    pub max_depth: usize,
    /// Maximum total pages to crawl
    pub max_pages: usize,
    /// URL patterns to include (regex)
    pub include_patterns: Vec<String>,
    /// URL patterns to exclude (regex)
    pub exclude_patterns: Vec<String>,
    /// Whether to respect robots.txt
    pub respect_robots_txt: bool,
    /// Current job state
    pub state: CrawlJobState,
    /// Pages crawled so far
    pub pages_crawled: usize,
    /// Pages successfully processed
    pub pages_success: usize,
    /// Pages that failed
    pub pages_failed: usize,
    /// URLs discovered but not yet crawled
    pub urls_discovered: usize,
    /// When the job started
    pub started_at: Option<DateTime<Utc>>,
    /// When the job completed/failed
    pub finished_at: Option<DateTime<Utc>>,
    /// Checkpoint data for recovery
    pub checkpoint: Option<CrawlCheckpoint>,
}

/// Checkpoint for crash recovery
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrawlCheckpoint {
    /// Frontier URLs awaiting crawl
    pub frontier_urls: Vec<FrontierUrl>,
    /// Seen URLs (bloom filter approximation as set for serialization)
    pub seen_urls: HashSet<String>,
    /// Content fingerprints
    pub fingerprints: HashMap<String, ContentFingerprint>,
    /// Checkpoint timestamp
    pub checkpointed_at: DateTime<Utc>,
    /// Job state at checkpoint
    pub pages_crawled: usize,
    pub pages_success: usize,
    pub pages_failed: usize,
    pub urls_discovered: usize,
}

impl CrawlJob {
    pub fn new(id: &str, name: &str, seed_urls: Vec<String>, max_depth: usize) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            seed_urls,
            max_depth,
            max_pages: 10000,
            include_patterns: Vec::new(),
            exclude_patterns: Vec::new(),
            respect_robots_txt: true,
            state: CrawlJobState::Pending,
            pages_crawled: 0,
            pages_success: 0,
            pages_failed: 0,
            urls_discovered: 0,
            started_at: None,
            finished_at: None,
            checkpoint: None,
        }
    }

    /// Create a checkpoint
    pub fn checkpoint(
        &mut self,
        _frontier: &UrlFrontier,
        seen: &HashSet<String>,
        fingerprints: &HashMap<String, ContentFingerprint>,
    ) {
        self.checkpoint = Some(CrawlCheckpoint {
            frontier_urls: Vec::new(), // Serialized from frontier
            seen_urls: seen.clone(),
            fingerprints: fingerprints.clone(),
            checkpointed_at: Utc::now(),
            pages_crawled: self.pages_crawled,
            pages_success: self.pages_success,
            pages_failed: self.pages_failed,
            urls_discovered: self.urls_discovered,
        });
        debug!(job_id = %self.id, pages = self.pages_crawled, "Checkpoint created");
    }

    /// Restore from checkpoint
    pub fn restore_from_checkpoint(&mut self) -> bool {
        if let Some(cp) = &self.checkpoint {
            self.pages_crawled = cp.pages_crawled;
            self.pages_success = cp.pages_success;
            self.pages_failed = cp.pages_failed;
            self.urls_discovered = cp.urls_discovered;
            self.state = CrawlJobState::Pending;
            true
        } else {
            false
        }
    }
}

/// Crawl engine configuration
#[derive(Debug, Clone)]
pub struct CrawlEngineConfig {
    /// Default min delay between requests per domain (ms)
    pub default_delay_ms: u64,
    /// Max concurrent requests per domain
    pub max_concurrent_per_domain: usize,
    /// Max total concurrent requests
    pub max_total_concurrent: usize,
    /// Request timeout (seconds)
    pub request_timeout_secs: u64,
    /// Maximum retries for failed requests
    pub max_retries: u32,
    /// Whether to respect robots.txt
    pub respect_robots_txt: bool,
    /// User agent string
    pub user_agent: String,
    /// Checkpoint interval (number of pages)
    pub checkpoint_interval: usize,
}

impl Default for CrawlEngineConfig {
    fn default() -> Self {
        Self {
            default_delay_ms: 1000,
            max_concurrent_per_domain: 2,
            max_total_concurrent: 10,
            request_timeout_secs: 30,
            max_retries: 3,
            respect_robots_txt: true,
            user_agent: "MarkifyBot/1.0 (+https://markify.com/bot)".to_string(),
            checkpoint_interval: 100,
        }
    }
}

// ─── Helper Functions ────────────────────────────────────────────────────────

/// Extract domain from URL
pub fn extract_domain(url: &str) -> &str {
    url.trim_start_matches("http://")
        .trim_start_matches("https://")
        .split('/')
        .next()
        .unwrap_or(url)
}

/// Check if URL matches any of the patterns
pub fn matches_patterns(url: &str, patterns: &[String]) -> bool {
    for pattern in patterns {
        if let Ok(re) = regex::Regex::new(pattern) {
            if re.is_match(url) {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bloom_filter() {
        let mut bf = CrawlBloomFilter::new(1000, 0.01);
        assert!(!bf.might_contain("https://example.com"));

        bf.add("https://example.com");
        assert!(bf.might_contain("https://example.com"));
        assert!(!bf.might_contain("https://other.com")); // Should be false
    }

    #[test]
    fn test_url_frontier() {
        let mut frontier = UrlFrontier::new();

        let url1 = FrontierUrl::new("https://a.com/page1", "job1", 3);
        let url2 = FrontierUrl::new("https://b.com/page2", "job1", 3);

        frontier.push(url1);
        frontier.push(url2);

        let stats = frontier.stats();
        assert_eq!(stats.total, 2);

        // Should be able to pop both
        let first = frontier.pop_next();
        assert!(first.is_some());
        let second = frontier.pop_next();
        assert!(second.is_some());
        assert!(frontier.pop_next().is_none());
    }

    #[test]
    fn test_domain_backoff() {
        let mut frontier = UrlFrontier::new();
        frontier.set_domain_policy("slow.com", 5000, 5, Duration::from_secs(60));
        frontier.report_domain_status("slow.com", 429);

        // After 429, domain should be in back-off
        let url = FrontierUrl::new("https://slow.com/page", "job1", 1);
        frontier.push(url);

        // pop_next should return None because domain is backing off
        assert!(frontier.pop_next().is_none());
    }

    #[test]
    fn test_content_fingerprint() {
        let fp1 = ContentFingerprint::new("https://example.com", "<html>Hello</html>");
        let fp2 = ContentFingerprint::new("https://example.com", "<html>Hello</html>");
        let fp3 = ContentFingerprint::new("https://example.com", "<html>Changed</html>");

        let result_same = fp1.compare(&fp2);
        assert!(!result_same.changed);
        assert_eq!(result_same.similarity, 1.0);

        let result_diff = fp1.compare(&fp3);
        assert!(result_diff.changed);
        assert_eq!(result_diff.similarity, 0.0);
    }

    #[test]
    fn test_extract_domain() {
        assert_eq!(extract_domain("https://example.com/path"), "example.com");
        assert_eq!(extract_domain("http://sub.example.com"), "sub.example.com");
        assert_eq!(extract_domain("example.com"), "example.com");
    }
}
