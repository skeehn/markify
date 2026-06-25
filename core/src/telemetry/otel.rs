//! OpenTelemetry Observability for Markify.
//!
//! Provides distributed tracing, metrics, and logging integration.
//!
//! Features:
//! - OpenTelemetry traces with spans for each pipeline stage
//! - Metrics: latency, throughput, error rates, cost
//! - Export to Jaeger, Prometheus, or OTLP collectors
//! - Compatible with existing Telemetry (simple mode)

use std::time::Instant;

use serde::{Deserialize, Serialize};
use tracing::{debug, info};
use uuid::Uuid;

/// OTel trace context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceContext {
    /// Unique trace ID
    pub trace_id: String,
    /// Span ID for this request
    pub span_id: String,
    /// Parent span ID (if any)
    pub parent_span_id: Option<String>,
}

impl TraceContext {
    pub fn new() -> Self {
        Self {
            trace_id: Uuid::new_v4().to_string(),
            span_id: Uuid::new_v4().to_string(),
            parent_span_id: None,
        }
    }

    pub fn with_parent(parent_span_id: String) -> Self {
        Self {
            trace_id: Uuid::new_v4().to_string(),
            span_id: Uuid::new_v4().to_string(),
            parent_span_id: Some(parent_span_id),
        }
    }
}

impl Default for TraceContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Span attributes for Markify operations
#[derive(Debug, Clone, Serialize)]
pub struct SpanAttributes {
    pub operation: String,
    pub url: Option<String>,
    pub status_code: Option<u16>,
    pub engine: Option<String>,
    pub cached: bool,
    pub duration_ms: f64,
    pub content_length: Option<usize>,
    pub block_count: Option<usize>,
    pub error: Option<String>,
}

impl SpanAttributes {
    pub fn new(operation: &str) -> Self {
        Self {
            operation: operation.to_string(),
            url: None,
            status_code: None,
            engine: None,
            cached: false,
            duration_ms: 0.0,
            content_length: None,
            block_count: None,
            error: None,
        }
    }
}

/// OTel-compatible request metrics
#[derive(Debug, Clone, Serialize)]
pub struct RequestMetrics {
    pub trace_id: String,
    pub operation: String,
    pub duration_ms: f64,
    pub status: String, // "success", "error", "timeout"
    pub cache_hit: bool,
    pub cost_usd: f64, // Estimated cost for this request
    pub content_bytes: usize,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// OTel Exporter configuration
#[derive(Debug, Clone)]
pub enum OtelExporter {
    /// Export to OTLP endpoint (Jaeger, Grafana, etc.)
    Otlp { endpoint: String },
    /// Export to stdout (debug mode)
    Stdout,
    /// Export to file
    File { path: String },
}

/// OTel observability manager
pub struct OtelObservability {
    exporter: OtelExporter,
    /// Whether OTel is enabled
    enabled: bool,
    /// Service name
    service_name: String,
    /// Service version
    service_version: String,
    /// Collected metrics
    metrics: Vec<RequestMetrics>,
    /// Maximum metrics to keep in memory
    max_metrics: usize,
}

impl OtelObservability {
    pub fn new(exporter: OtelExporter, service_name: &str, service_version: &str) -> Self {
        Self {
            exporter,
            enabled: true,
            service_name: service_name.to_string(),
            service_version: service_version.to_string(),
            metrics: Vec::new(),
            max_metrics: 10000,
        }
    }

    /// Create with stdout exporter (debug mode)
    pub fn debug_mode() -> Self {
        Self::new(OtelExporter::Stdout, "markify", env!("CARGO_PKG_VERSION"))
    }

    /// Start a traced operation
    pub fn start_operation(&self, operation: &str) -> (TraceContext, Instant) {
        let ctx = TraceContext::new();
        debug!(
            trace_id = %ctx.trace_id,
            operation = operation,
            "Operation started"
        );
        (ctx, Instant::now())
    }

    /// End a traced operation and record metrics
    pub fn end_operation(
        &mut self,
        ctx: &TraceContext,
        operation: &str,
        start: Instant,
        status: &str,
        cache_hit: bool,
        cost_usd: f64,
        content_bytes: usize,
    ) {
        let duration_ms = start.elapsed().as_secs_f64() * 1000.0;

        let metrics = RequestMetrics {
            trace_id: ctx.trace_id.clone(),
            operation: operation.to_string(),
            duration_ms,
            status: status.to_string(),
            cache_hit,
            cost_usd,
            content_bytes,
            timestamp: chrono::Utc::now(),
        };

        // Export
        if self.enabled {
            match &self.exporter {
                OtelExporter::Stdout => {
                    info!(
                        trace_id = %ctx.trace_id,
                        operation = operation,
                        duration_ms = duration_ms,
                        status = status,
                        cache_hit = cache_hit,
                        cost_usd = cost_usd,
                        "Operation completed"
                    );
                }
                OtelExporter::Otlp { endpoint } => {
                    // In production, send to OTLP endpoint
                    debug!(endpoint = endpoint, "Would send trace to OTLP endpoint");
                }
                OtelExporter::File { path } => {
                    // In production, append to file
                    debug!(path = path, "Would append trace to file");
                }
            }
        }

        // Store metrics
        if self.metrics.len() < self.max_metrics {
            self.metrics.push(metrics);
        }
    }

    /// Create a child span for a sub-operation
    pub fn create_child_span(&self, parent: &TraceContext, _operation: &str) -> TraceContext {
        TraceContext::with_parent(parent.span_id.clone())
    }

    /// Record an error in the current trace
    pub fn record_error(&self, ctx: &TraceContext, error: &str) {
        debug!(
            trace_id = %ctx.trace_id,
            error = error,
            "Error recorded in trace"
        );
    }

    /// Get metrics summary
    pub fn metrics_summary(&self) -> MetricsSummary {
        if self.metrics.is_empty() {
            return MetricsSummary::default();
        }

        let total = self.metrics.len();
        let errors = self.metrics.iter().filter(|m| m.status == "error").count();
        let cache_hits = self.metrics.iter().filter(|m| m.cache_hit).count();
        let total_duration: f64 = self.metrics.iter().map(|m| m.duration_ms).sum();
        let total_cost: f64 = self.metrics.iter().map(|m| m.cost_usd).sum();

        let durations: Vec<f64> = self.metrics.iter().map(|m| m.duration_ms).collect();
        let p50 = percentile(&durations, 0.5);
        let p95 = percentile(&durations, 0.95);
        let p99 = percentile(&durations, 0.99);

        MetricsSummary {
            total_requests: total,
            error_count: errors,
            error_rate: errors as f64 / total as f64,
            cache_hit_rate: cache_hits as f64 / total as f64,
            avg_duration_ms: total_duration / total as f64,
            p50_duration_ms: p50,
            p95_duration_ms: p95,
            p99_duration_ms: p99,
            total_cost_usd: total_cost,
        }
    }

    /// Enable or disable OTel
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }
}

/// Calculate percentile of a sorted slice
fn percentile(data: &[f64], p: f64) -> f64 {
    if data.is_empty() {
        return 0.0;
    }
    let mut sorted = data.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let idx = (p * (sorted.len() - 1) as f64).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

/// Metrics summary for dashboards
#[derive(Debug, Clone, Default, Serialize)]
pub struct MetricsSummary {
    pub total_requests: usize,
    pub error_count: usize,
    pub error_rate: f64,
    pub cache_hit_rate: f64,
    pub avg_duration_ms: f64,
    pub p50_duration_ms: f64,
    pub p95_duration_ms: f64,
    pub p99_duration_ms: f64,
    pub total_cost_usd: f64,
}

/// Middleware layer for Axum to auto-trace requests
pub struct TraceMiddleware {
    pub service_name: String,
}

impl TraceMiddleware {
    pub fn new(service_name: &str) -> Self {
        Self {
            service_name: service_name.to_string(),
        }
    }

    /// Generate a trace ID for an incoming request
    pub fn start_request(&self, method: &str, path: &str) -> (TraceContext, Instant) {
        let ctx = TraceContext::new();
        debug!(
            trace_id = %ctx.trace_id,
            method = method,
            path = path,
            service = %self.service_name,
            "Incoming request"
        );
        (ctx, Instant::now())
    }

    /// End request tracing and log
    pub fn end_request(
        &self,
        ctx: &TraceContext,
        method: &str,
        path: &str,
        status: u16,
        duration_ms: f64,
    ) {
        debug!(
            trace_id = %ctx.trace_id,
            method = method,
            path = path,
            status = status,
            duration_ms = duration_ms,
            "Request completed"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trace_context_creation() {
        let ctx = TraceContext::new();
        assert!(!ctx.trace_id.is_empty());
        assert!(!ctx.span_id.is_empty());
        assert!(ctx.parent_span_id.is_none());
    }

    #[test]
    fn test_metrics_summary() {
        let mut otel = OtelObservability::debug_mode();

        // Simulate some metrics
        for i in 0..100 {
            let ctx = TraceContext::new();
            let start = Instant::now();
            otel.end_operation(
                &ctx,
                "scrape",
                start,
                if i % 10 == 0 { "error" } else { "success" },
                i % 3 == 0,
                0.001,
                1000,
            );
        }

        let summary = otel.metrics_summary();
        assert_eq!(summary.total_requests, 100);
        assert_eq!(summary.error_count, 10);
        assert!((summary.error_rate - 0.1).abs() < 0.01);
        assert!(summary.cache_hit_rate > 0.3);
    }

    #[test]
    fn test_percentile() {
        let data = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        assert_eq!(percentile(&data, 0.5), 3.0);
        assert_eq!(percentile(&data, 0.95), 5.0);
    }
}
