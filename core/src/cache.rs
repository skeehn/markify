//! In-memory LRU cache with TTL support.
//!
//! Uses moka for high-performance caching with an upgrade path to Redis.

use moka::future::Cache;
use std::time::Duration;
use tracing::debug;

/// Cache configuration
#[derive(Debug, Clone)]
pub struct CacheConfig {
    /// Maximum number of entries
    pub max_capacity: u64,
    /// Time-to-live for entries
    pub ttl: Duration,
    /// Time-to-idle before eviction
    pub tti: Option<Duration>,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            max_capacity: 10_000,
            ttl: Duration::from_secs(3600),       // 1 hour
            tti: Some(Duration::from_secs(1800)), // 30 min idle
        }
    }
}

/// A cached entry with metadata
#[derive(Debug, Clone)]
pub struct CachedEntry {
    pub data: Vec<u8>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub accessed_count: u64,
}

/// Markify's cache layer
pub struct MarkifyCache {
    cache: Cache<String, CachedEntry>,
    config: CacheConfig,
}

impl MarkifyCache {
    pub fn new(config: CacheConfig) -> Self {
        let cache = Cache::builder()
            .max_capacity(config.max_capacity)
            .time_to_live(config.ttl)
            .time_to_idle(config.tti.unwrap_or(Duration::from_secs(1800)))
            .build();

        debug!(
            capacity = config.max_capacity,
            ttl_secs = config.ttl.as_secs(),
            "Cache initialized"
        );

        Self { cache, config }
    }

    /// Get a cached entry by key (URL hash)
    pub async fn get(&self, key: &str) -> Option<CachedEntry> {
        self.cache.get(key).await
    }

    /// Insert an entry into the cache
    pub async fn insert(&self, key: String, data: Vec<u8>) {
        let entry = CachedEntry {
            data,
            created_at: chrono::Utc::now(),
            accessed_count: 0,
        };
        self.cache.insert(key, entry).await;
    }

    /// Check if a key exists in cache
    pub async fn contains_key(&self, key: &str) -> bool {
        self.cache.contains_key(key)
    }

    /// Remove a specific key
    pub fn invalidate(&self, key: &str) {
        self.cache.invalidate(key);
    }

    /// Clear all entries
    pub fn invalidate_all(&self) {
        self.cache.invalidate_all();
    }

    /// Get cache statistics
    pub fn stats(&self) -> CacheStats {
        CacheStats {
            entry_count: self.cache.entry_count(),
            weighted_size: self.cache.weighted_size(),
        }
    }

    /// Generate a cache key from a URL + options
    pub fn make_key(url: &str, mode: &str, format: &str) -> String {
        format!("markify:{}:{}:{}", mode, format, url)
    }
}

#[derive(Debug)]
pub struct CacheStats {
    pub entry_count: u64,
    pub weighted_size: u64,
}

impl std::fmt::Display for CacheStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Cache {{ entries: {}, size: {} }}",
            self.entry_count, self.weighted_size
        )
    }
}
