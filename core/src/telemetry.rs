//! Telemetry: request metrics collection and health stats.
//!
//! Tracks: total requests, success/error counts, latencies, cache hit rate,
//! engine distribution, and per-endpoint stats.
//!
//! Phase 2: OpenTelemetry integration for distributed tracing.

pub mod otel;

pub use otel::{MetricsSummary, OtelExporter, OtelObservability, TraceContext, TraceMiddleware};

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

/// Shared telemetry state
#[derive(Clone)]
pub struct Telemetry {
    /// Total requests
    total_requests: Arc<AtomicU64>,
    /// Successful requests
    success_count: Arc<AtomicU64>,
    /// Failed requests
    error_count: Arc<AtomicU64>,
    /// Cache hits
    cache_hits: Arc<AtomicU64>,
    /// HTTP engine usage
    http_engine_count: Arc<AtomicU64>,
    /// Browser engine usage
    browser_engine_count: Arc<AtomicU64>,
    /// Total processing time (ms)
    total_processing_ms: Arc<AtomicU64>,
    /// Started at
    started_at: Instant,
}

impl Telemetry {
    pub fn new() -> Self {
        Self {
            total_requests: Arc::new(AtomicU64::new(0)),
            success_count: Arc::new(AtomicU64::new(0)),
            error_count: Arc::new(AtomicU64::new(0)),
            cache_hits: Arc::new(AtomicU64::new(0)),
            http_engine_count: Arc::new(AtomicU64::new(0)),
            browser_engine_count: Arc::new(AtomicU64::new(0)),
            total_processing_ms: Arc::new(AtomicU64::new(0)),
            started_at: Instant::now(),
        }
    }

    /// Record a successful request
    pub fn record_success(&self, processing_ms: u64, cached: bool, engine: &str) {
        self.total_requests.fetch_add(1, Ordering::Relaxed);
        self.success_count.fetch_add(1, Ordering::Relaxed);
        self.total_processing_ms
            .fetch_add(processing_ms, Ordering::Relaxed);
        if cached {
            self.cache_hits.fetch_add(1, Ordering::Relaxed);
        }
        match engine {
            "http" => {
                self.http_engine_count.fetch_add(1, Ordering::Relaxed);
            }
            "browser" => {
                self.browser_engine_count.fetch_add(1, Ordering::Relaxed);
            }
            _ => {}
        };
    }

    /// Record a failed request
    pub fn record_error(&self) {
        self.total_requests.fetch_add(1, Ordering::Relaxed);
        self.error_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Get current stats as JSON
    pub fn stats(&self) -> serde_json::Value {
        let total = self.total_requests.load(Ordering::Relaxed);
        let success = self.success_count.load(Ordering::Relaxed);
        let errors = self.error_count.load(Ordering::Relaxed);
        let cache_hits = self.cache_hits.load(Ordering::Relaxed);
        let http = self.http_engine_count.load(Ordering::Relaxed);
        let browser = self.browser_engine_count.load(Ordering::Relaxed);
        let total_ms = self.total_processing_ms.load(Ordering::Relaxed);
        let uptime_secs = self.started_at.elapsed().as_secs();

        let avg_ms = if total > 0 { total_ms / total } else { 0 };

        serde_json::json!({
            "requests": {
                "total": total,
                "success": success,
                "errors": errors,
                "success_rate": if total > 0 { (success as f64 / total as f64 * 100.0).round() } else { 0.0 },
            },
            "performance": {
                "avg_latency_ms": avg_ms,
                "total_processing_ms": total_ms,
            },
            "cache": {
                "hits": cache_hits,
                "hit_rate": if total > 0 { (cache_hits as f64 / total as f64 * 100.0).round() } else { 0.0 },
            },
            "engines": {
                "http": http,
                "browser": browser,
            },
            "uptime": {
                "seconds": uptime_secs,
                "formatted": format_uptime(uptime_secs),
            }
        })
    }
}

impl Default for Telemetry {
    fn default() -> Self {
        Self::new()
    }
}

fn format_uptime(secs: u64) -> String {
    let days = secs / 86400;
    let hours = (secs % 86400) / 3600;
    let minutes = (secs % 3600) / 60;
    let seconds = secs % 60;

    if days > 0 {
        format!("{}d {}h {}m {}s", days, hours, minutes, seconds)
    } else if hours > 0 {
        format!("{}h {}m {}s", hours, minutes, seconds)
    } else if minutes > 0 {
        format!("{}m {}s", minutes, seconds)
    } else {
        format!("{}s", seconds)
    }
}
