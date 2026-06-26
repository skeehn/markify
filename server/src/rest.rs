//! REST API routes for Markify.

use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing::info;

use markify_core::{
    crawl::{ContentFingerprint, CrawlBloomFilter, FrontierUrl, UrlFrontier},
    crawl::{CrawlJob, CrawlPageResult, CrawlStatus},
    structured_api::spec::{ApiSpec, ExecuteApiRequest, GenerateApiRequest},
    vsb_graph::segment_page,
    CacheConfig, CilowClient, DenseIndex, ExaClient, ExtractionMode, FetchConfig, HybridSearcher,
    Markify, OutputFormat, RrfConfig, ScrapeMeta, ScrapeRequest, ScrapeResult, SearchClient,
    SearchConfig, SparseIndex, Telemetry,
};
use std::collections::HashMap;
use std::sync::RwLock;

/// Application state
pub struct AppState {
    pub client: Markify,
    pub search: Option<SearchClient>,
    pub exa: Option<ExaClient>,
    pub cilow: Option<CilowClient>,
    pub telemetry: Telemetry,
    pub api_specs: Arc<RwLock<HashMap<String, ApiSpec>>>,
    pub sparse_index: Arc<RwLock<SparseIndex>>,
    pub dense_index: Arc<RwLock<DenseIndex>>,
    pub hybrid_searcher: Arc<RwLock<Option<HybridSearcher>>>,
    // Crawl engine state
    pub crawl_jobs: Arc<RwLock<HashMap<String, CrawlJob>>>,
    pub crawl_frontier: Arc<RwLock<UrlFrontier>>,
    pub crawl_bloom: Arc<RwLock<CrawlBloomFilter>>,
    pub crawl_results: Arc<RwLock<HashMap<String, Vec<CrawlPageResult>>>>,
    pub crawl_fingerprints: Arc<RwLock<HashMap<String, ContentFingerprint>>>,
}

/// Create the Axum router with all API routes
pub fn create_router() -> Router {
    let search_config = SearchConfig::default();
    // Always available: Serper (Google) when SERPER_API_KEY is set, otherwise
    // keyless DuckDuckGo HTML search so /v1/search works with no API key.
    let search_client = {
        let client = SearchClient::new(search_config.api_key);
        match client.backend_name() {
            "serper" => info!("Search enabled via Serper (Google)"),
            _ => info!(
                "Search enabled via DuckDuckGo (keyless; set SERPER_API_KEY for Google results)"
            ),
        }
        Some(client)
    };

    // Exa neural search
    let exa_client = ExaClient::from_env();
    if exa_client.is_some() {
        info!("Neural search enabled (Exa API key found)");
    } else {
        info!("Neural search disabled (set EXA_API_KEY to enable)");
    }

    // Cilow integration
    let cilow_client = CilowClient::from_env();
    if cilow_client.is_some() {
        info!("Cilow export enabled (CILOW_API_URL found)");
    } else {
        info!("Cilow export disabled (set CILOW_API_URL to enable)");
    }

    let state = Arc::new(AppState {
        client: Markify::new(FetchConfig::default(), CacheConfig::default()),
        search: search_client,
        exa: exa_client,
        cilow: cilow_client,
        telemetry: Telemetry::new(),
        // Load any previously-generated API specs from disk so they survive restarts.
        api_specs: Arc::new(RwLock::new(markify_core::structured_api::store::load_all())),
        sparse_index: Arc::new(RwLock::new(
            SparseIndex::new_in_memory().expect("Failed to create sparse index"),
        )),
        dense_index: Arc::new(RwLock::new(DenseIndex::new(384))),
        hybrid_searcher: Arc::new(RwLock::new(None)),
        // Crawl engine state
        crawl_jobs: Arc::new(RwLock::new(HashMap::new())),
        crawl_frontier: Arc::new(RwLock::new(UrlFrontier::new())),
        crawl_bloom: Arc::new(RwLock::new(CrawlBloomFilter::new(100000, 0.01))),
        crawl_results: Arc::new(RwLock::new(HashMap::new())),
        crawl_fingerprints: Arc::new(RwLock::new(HashMap::new())),
    });

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        // Health
        .route("/health", get(health_handler))
        .route("/api/health", get(health_handler))
        // v1 API — Scraping
        .route("/v1/scrape", post(scrape_handler))
        .route("/v1/search", post(search_handler))
        .route("/v1/batch", post(batch_handler))
        .route("/v1/metadata", get(metadata_handler))
        // v1 API — Structured API (Parse.bot-style)
        .route("/v1/generate", post(generate_api_handler))
        .route("/v1/apis", get(list_apis_handler))
        .route("/v1/apis/:id/execute", post(execute_api_handler))
        .route("/v1/apis/:id", get(get_api_handler))
        // v1 API — Neural Search (Exa)
        .route("/v1/neural-search", post(neural_search_handler))
        // v1 API — Cilow Export
        .route("/v1/export/cilow", post(export_cilow_handler))
        // v1 API — VSB-Graph (Asterism)
        .route("/v1/vsb", post(vsb_handler))
        // v1 API — Sparse Index Search (Tantivy BM25)
        .route("/v1/search-index", get(search_index_handler))
        // v1 API — Dense Index Search (Neural/Cosine)
        .route("/v1/neural-index", post(neural_index_handler))
        // v1 API — Hybrid Search (BM25 + Dense RRF fusion)
        .route("/v1/hybrid-search", post(hybrid_index_handler))
        // v1 API — Crawl Engine
        .route("/v1/crawl/start", post(crawl_start_handler))
        .route("/v1/crawl/status", get(crawl_status_handler))
        .route("/v1/crawl/stop", post(crawl_stop_handler))
        .route("/v1/crawl/jobs", get(crawl_list_jobs_handler))
        .route("/v1/crawl/results", get(crawl_results_handler))
        // Health
        .route("/v1/health", get(health_v1_handler))
        // Legacy compatibility
        .route("/api/convert", post(legacy_convert_handler))
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

// ─── Handlers ───────────────────────────────────────────────────────────────

/// Health check
async fn health_handler() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "ok",
        "service": "markify",
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

/// v1 health check with real stats
async fn health_v1_handler(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let telem_stats = state.telemetry.stats();

    Json(serde_json::json!({
        "status": "ok",
        "service": "markify",
        "version": env!("CARGO_PKG_VERSION"),
        "telemetry": telem_stats,
    }))
}

/// POST /v1/scrape — Scrape a URL and return content
async fn scrape_handler(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ScrapeRequestV1>,
) -> (StatusCode, Json<ScrapeResponseV1>) {
    if req.url.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(ScrapeResponseV1::error("URL cannot be empty".to_string())),
        );
    }

    // Parse output formats
    let formats: Vec<OutputFormat> = req
        .formats
        .as_ref()
        .unwrap_or(&vec!["both".to_string()])
        .iter()
        .filter_map(|f| match f.as_str() {
            "markdown" => Some(OutputFormat::Markdown),
            "json" => Some(OutputFormat::Json),
            "both" => Some(OutputFormat::Both),
            _ => None,
        })
        .collect();

    // Parse extraction mode
    let mode = req
        .mode
        .as_ref()
        .map(|m| match m.as_str() {
            "article" => ExtractionMode::Article,
            "full" => ExtractionMode::Full,
            "links" => ExtractionMode::Links,
            "images" => ExtractionMode::Images,
            "metadata" => ExtractionMode::Metadata,
            _ => ExtractionMode::Smart,
        })
        .unwrap_or_default();

    let scrape_req = ScrapeRequest {
        url: req.url.clone(),
        formats,
        mode,
        wait_for_selector: req.wait_for_selector.clone(),
        timeout_ms: req.timeout_ms,
        force_browser: req.force_browser.unwrap_or(false),
        include_raw_html: req.include_raw_html.unwrap_or(false),
        include_links: req.include_links.unwrap_or(false),
        include_images: req.include_images.unwrap_or(false),
        extract_schema: req.extract_schema.clone(),
    };

    match state.client.scrape(scrape_req).await {
        Ok((result, meta)) => {
            state
                .telemetry
                .record_success(meta.total_ms, meta.cached, &meta.engine);
            (
                StatusCode::OK,
                Json(ScrapeResponseV1::success(result, meta)),
            )
        }
        Err(e) => {
            state.telemetry.record_error();
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ScrapeResponseV1::error(e.to_string())),
            )
        }
    }
}

/// POST /v1/batch — Scrape multiple URLs
async fn batch_handler(
    State(state): State<Arc<AppState>>,
    Json(req): Json<BatchRequestV1>,
) -> (StatusCode, Json<serde_json::Value>) {
    if req.urls.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "success": false,
                "error": "URLs cannot be empty",
            })),
        );
    }

    // Limit batch size
    if req.urls.len() > 100 {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "success": false,
                "error": "Maximum 100 URLs per batch",
            })),
        );
    }

    let mut results = Vec::new();

    for url in &req.urls {
        let scrape_req = ScrapeRequest {
            url: url.clone(),
            formats: vec![OutputFormat::Both],
            mode: ExtractionMode::Smart,
            ..Default::default()
        };

        match state.client.scrape(scrape_req).await {
            Ok((result, meta)) => {
                results.push(serde_json::json!({
                    "url": url,
                    "success": true,
                    "status_code": result.status_code,
                    "markdown": result.markdown,
                    "metadata": result.metadata,
                    "fetch_ms": meta.fetch_ms,
                    "engine": meta.engine,
                }));
            }
            Err(e) => {
                results.push(serde_json::json!({
                    "url": url,
                    "success": false,
                    "error": e.to_string(),
                }));
            }
        }
    }

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "success": true,
            "total": req.urls.len(),
            "results": results,
        })),
    )
}

/// GET /v1/metadata — Lightweight metadata extraction
async fn metadata_handler(
    State(state): State<Arc<AppState>>,
    axum::extract::Query(params): axum::extract::Query<MetadataQueryParams>,
) -> (StatusCode, Json<serde_json::Value>) {
    if params.url.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "success": false,
                "error": "URL parameter is required",
            })),
        );
    }

    let scrape_req = ScrapeRequest {
        url: params.url.clone(),
        formats: vec![OutputFormat::Json],
        mode: ExtractionMode::Metadata,
        timeout_ms: Some(10000), // 10s timeout for metadata
        ..Default::default()
    };

    match state.client.scrape(scrape_req).await {
        Ok((result, meta)) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "success": true,
                "url": params.url,
                "metadata": result.metadata,
                "fetch_ms": meta.fetch_ms,
            })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "success": false,
                "error": e.to_string(),
            })),
        ),
    }
}

/// Legacy POST /api/convert — Backward compatibility with old Markify
async fn legacy_convert_handler(
    State(_state): State<Arc<AppState>>,
    Json(req): Json<LegacyConvertRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    use markify_core::transform::markdown::to_markdown;

    let markdown = to_markdown(&req.html);

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "markdown": markdown,
            "success": true,
        })),
    )
}

// ─── Request/Response Types ─────────────────────────────────────────────────

#[derive(Debug, serde::Deserialize)]
pub struct ScrapeRequestV1 {
    pub url: String,
    pub formats: Option<Vec<String>>,
    pub mode: Option<String>,
    pub wait_for_selector: Option<String>,
    pub timeout_ms: Option<u64>,
    pub force_browser: Option<bool>,
    pub include_raw_html: Option<bool>,
    pub include_links: Option<bool>,
    pub include_images: Option<bool>,
    pub extract_schema: Option<serde_json::Value>,
}

#[derive(Debug, serde::Serialize)]
pub struct ScrapeResponseV1 {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<ScrapeResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<ScrapeMeta>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl ScrapeResponseV1 {
    pub fn success(data: ScrapeResult, meta: ScrapeMeta) -> Self {
        Self {
            success: true,
            data: Some(data),
            meta: Some(meta),
            error: None,
        }
    }

    pub fn error(message: String) -> Self {
        Self {
            success: false,
            data: None,
            meta: None,
            error: Some(message),
        }
    }
}

#[derive(Debug, serde::Deserialize)]
pub struct BatchRequestV1 {
    pub urls: Vec<String>,
}

#[derive(Debug, serde::Deserialize)]
pub struct MetadataQueryParams {
    pub url: String,
}

#[derive(Debug, serde::Deserialize)]
pub struct LegacyConvertRequest {
    pub html: String,
}

// ─── Search Handler ─────────────────────────────────────────────────────────

#[derive(Debug, serde::Deserialize)]
pub struct SearchRequestV1 {
    pub query: String,
    pub num_results: Option<usize>,
    pub scrape_results: Option<bool>,
}

async fn search_handler(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SearchRequestV1>,
) -> (StatusCode, Json<serde_json::Value>) {
    if req.query.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "success": false,
                "error": "Query cannot be empty",
            })),
        );
    }

    let Some(search_client) = &state.search else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "success": false,
                "error": "Search not configured. Set SERPER_API_KEY environment variable.",
            })),
        );
    };

    let num = req.num_results.unwrap_or(5);

    // If scrape_results is true, scrape each search result
    if req.scrape_results.unwrap_or(false) {
        match search_client
            .search_and_scrape(&req.query, num, &state.client)
            .await
        {
            Ok(results) => (
                StatusCode::OK,
                Json(serde_json::json!({
                    "success": true,
                    "query": req.query,
                    "results": results,
                })),
            ),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "success": false,
                    "error": e.to_string(),
                })),
            ),
        }
    } else {
        // Just search, no scraping
        match search_client.search(&req.query, num).await {
            Ok(results) => (
                StatusCode::OK,
                Json(serde_json::json!({
                    "success": true,
                    "query": results.query,
                    "count": results.count,
                    "results": results.results,
                })),
            ),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "success": false,
                    "error": e.to_string(),
                })),
            ),
        }
    }
}

// ─── Structured API Handlers (Parse.bot-style) ──────────────────────────────

async fn generate_api_handler(
    State(state): State<Arc<AppState>>,
    Json(req): Json<GenerateApiRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    if req.url.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "success": false,
                "error": "URL cannot be empty",
            })),
        );
    }

    match markify_core::generate_api_spec(&req.url, req.description.as_deref(), &state.client).await
    {
        Ok(spec) => {
            let id = spec.id.clone();
            // Store the spec in memory and persist it to disk so it survives restarts.
            if let Ok(mut specs) = state.api_specs.write() {
                specs.insert(id.clone(), spec.clone());
            }
            markify_core::structured_api::store::save_spec(&spec);

            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "success": true,
                    "api": spec,
                    "message": format!("API spec generated with ID: {}", id),
                })),
            )
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "success": false,
                "error": e.to_string(),
            })),
        ),
    }
}

async fn list_apis_handler(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let specs = if let Ok(specs) = state.api_specs.read() {
        specs
            .values()
            .map(|s| {
                serde_json::json!({
                    "id": s.id,
                    "name": s.name,
                    "url": s.url,
                    "description": s.description,
                    "endpoints": s.endpoints.len(),
                    "status": s.status,
                    "created_at": s.created_at,
                })
            })
            .collect::<Vec<_>>()
    } else {
        vec![]
    };

    Json(serde_json::json!({
        "success": true,
        "count": specs.len(),
        "apis": specs,
    }))
}

async fn get_api_handler(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let spec = if let Ok(specs) = state.api_specs.read() {
        specs.get(&id).cloned()
    } else {
        None
    };

    match spec {
        Some(s) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "success": true,
                "api": s,
            })),
        ),
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "success": false,
                "error": format!("API spec '{}' not found", id),
            })),
        ),
    }
}

async fn execute_api_handler(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(id): axum::extract::Path<String>,
    Json(req): Json<ExecuteApiRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    // Get the spec
    let spec = if let Ok(specs) = state.api_specs.read() {
        specs.get(&id).cloned()
    } else {
        None
    };

    let Some(spec) = spec else {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "success": false,
                "error": format!("API spec '{}' not found", id),
            })),
        );
    };

    // Find the first endpoint or use the one specified
    let endpoint_name = req
        .params
        .as_ref()
        .and_then(|p| p.get("endpoint").and_then(|v| v.as_str()))
        .unwrap_or_else(|| {
            spec.endpoints
                .first()
                .map(|e| e.name.as_str())
                .unwrap_or("")
        });

    if endpoint_name.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "success": false,
                "error": "No endpoints available in this API spec",
            })),
        );
    }

    let max_pages = req.max_pages.unwrap_or(1);
    match markify_core::execute_api_spec(
        &spec,
        endpoint_name,
        &state.client,
        req.url.as_deref(),
        max_pages,
    )
    .await
    {
        Ok(result) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "success": true,
                "result": result,
            })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "success": false,
                "error": e.to_string(),
            })),
        ),
    }
}

// ─── Neural Search Handler (Exa) ────────────────────────────────────────────

#[derive(Debug, serde::Deserialize)]
pub struct NeuralSearchRequest {
    pub query: String,
    pub num_results: Option<usize>,
    pub scrape_results: Option<bool>,
}

async fn neural_search_handler(
    State(state): State<Arc<AppState>>,
    Json(req): Json<NeuralSearchRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    if req.query.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "success": false,
                "error": "Query cannot be empty",
            })),
        );
    }

    let Some(exa) = &state.exa else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "success": false,
                "error": "Neural search not configured. Set EXA_API_KEY environment variable.",
            })),
        );
    };

    let num = req.num_results.unwrap_or(5);
    let scrape = req.scrape_results.unwrap_or(false);

    if scrape {
        match exa.search_and_scrape(&req.query, num, &state.client).await {
            Ok(results) => (
                StatusCode::OK,
                Json(serde_json::json!({
                    "success": true,
                    "query": req.query,
                    "results": results,
                })),
            ),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "success": false,
                    "error": e.to_string(),
                })),
            ),
        }
    } else {
        match exa.search(&req.query, num).await {
            Ok(results) => (
                StatusCode::OK,
                Json(serde_json::json!({
                    "success": true,
                    "query": results.query,
                    "count": results.count,
                    "results": results.results,
                })),
            ),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "success": false,
                    "error": e.to_string(),
                })),
            ),
        }
    }
}

// ─── Cilow Export Handler ───────────────────────────────────────────────────

#[derive(Debug, serde::Deserialize)]
pub struct CilowExportRequest {
    pub url: String,
    pub tags: Option<Vec<String>>,
    pub mode: Option<String>,
}

async fn export_cilow_handler(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CilowExportRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    if req.url.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "success": false,
                "error": "URL cannot be empty",
            })),
        );
    }

    let Some(cilow) = &state.cilow else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "success": false,
                "error": "Cilow export not configured. Set CILOW_API_URL environment variable.",
            })),
        );
    };

    let mode = req.mode.as_deref().unwrap_or("smart");
    let extraction_mode = match mode {
        "article" => ExtractionMode::Article,
        "full" => ExtractionMode::Full,
        _ => ExtractionMode::Smart,
    };

    let scrape_req = ScrapeRequest {
        url: req.url.clone(),
        formats: vec![OutputFormat::Markdown],
        mode: extraction_mode,
        include_raw_html: true,
        ..Default::default()
    };

    match state.client.scrape(scrape_req).await {
        Ok((result, meta)) => {
            match cilow
                .export_document(&result, &meta, req.tags.clone())
                .await
            {
                Ok(export_result) => (
                    StatusCode::OK,
                    Json(serde_json::json!({
                        "success": true,
                        "scrape": {
                            "url": req.url,
                            "engine": meta.engine,
                            "fetch_ms": meta.fetch_ms,
                            "total_ms": meta.total_ms,
                        },
                        "cilow": export_result,
                    })),
                ),
                Err(e) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({
                        "success": false,
                        "error": format!("Scraped but failed to export to Cilow: {}", e),
                    })),
                ),
            }
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "success": false,
                "error": format!("Failed to scrape: {}", e),
            })),
        ),
    }
}

// ─── VSB-Graph Handler (Asterism) ──────────────────────────────────────────

#[derive(Debug, serde::Deserialize)]
pub struct VsbRequest {
    pub url: String,
    /// Output format: markdown, json, both
    pub format: Option<String>,
    /// Whether to index blocks into the sparse BM25 index
    #[serde(default)]
    pub index: bool,
}

async fn vsb_handler(
    State(state): State<Arc<AppState>>,
    Json(req): Json<VsbRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    if req.url.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "success": false,
                "error": "URL cannot be empty",
            })),
        );
    }

    // Scrape the page first
    let scrape_req = ScrapeRequest {
        url: req.url.clone(),
        formats: vec![OutputFormat::Markdown],
        mode: ExtractionMode::Full,
        include_raw_html: true,
        ..Default::default()
    };

    match state.client.scrape(scrape_req).await {
        Ok((result, meta)) => {
            state
                .telemetry
                .record_success(meta.total_ms, meta.cached, &meta.engine);

            let html = result.raw_html.as_deref().unwrap_or("");

            // Segment into VSB-Graph
            let graph = segment_page(html, &req.url);

            // Index blocks into sparse index if requested
            let mut indexed_count = 0;
            if req.index {
                // Sparse BM25 index with fielded boosts
                if let Ok(index) = state.sparse_index.read() {
                    for (block_id, block) in &graph.blocks {
                        // Extract headers from text (lines that look like headings)
                        let lines: Vec<&str> = block.text.lines().collect();
                        let headers: Vec<&str> = lines
                            .iter()
                            .filter(|l| l.len() < 100 && (l.len() < 60 || l.ends_with(':')))
                            .copied()
                            .collect();
                        let headers_text = headers.join(" ");

                        // Metadata from block type and semantic role
                        let metadata = format!(
                            "{} {}",
                            format!("{:?}", block.block_type),
                            format!("{:?}", block.semantic_role),
                        );

                        let _ = index.index_block(
                            block_id,
                            &req.url,
                            graph.page_title.as_deref().unwrap_or(""),
                            &headers_text,
                            &block.text,
                            &metadata,
                            &format!("{:?}", block.block_type),
                        );
                        indexed_count += 1;
                    }
                }

                // Dense vector index
                if let Ok(mut dense) = state.dense_index.write() {
                    let di: &mut DenseIndex = &mut dense;
                    for (block_id, block) in &graph.blocks {
                        di.add_entry(
                            block_id,
                            &req.url,
                            graph.page_title.as_deref().unwrap_or(""),
                            &block.text,
                            &format!("{:?}", block.block_type),
                        );
                    }
                    // Rebuild vocabulary with new entries
                    di.build_vocab();
                }
            }

            let output_format = req.format.as_deref().unwrap_or("both");
            let markdown = graph.to_markdown();
            let json_output = graph.to_json();

            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "success": true,
                    "graph": {
                        "page_url": graph.page_url,
                        "page_title": graph.page_title,
                        "page_language": graph.page_language,
                        "total_text_length": graph.total_text_length,
                        "content_blocks": graph.content_block_count,
                        "boilerplate_blocks": graph.boilerplate_block_count,
                        "total_blocks": graph.blocks.len(),
                        "indexed_blocks": indexed_count,
                    },
                    "markdown": if output_format == "markdown" || output_format == "both" { Some(markdown) } else { None },
                    "blocks": if output_format == "json" || output_format == "both" { Some(json_output["blocks"].clone()) } else { None },
                    "meta": {
                        "engine": meta.engine,
                        "fetch_ms": meta.fetch_ms,
                    }
                })),
            )
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "success": false,
                "error": format!("Failed to scrape: {}", e),
            })),
        ),
    }
}

// ─── Sparse Index Search Handler (Tantivy BM25) ────────────────────────────

use axum::extract::Query;

#[derive(Debug, serde::Deserialize)]
pub struct SearchIndexQuery {
    pub q: String,
    pub limit: Option<usize>,
}

async fn search_index_handler(
    State(state): State<Arc<AppState>>,
    Query(params): Query<SearchIndexQuery>,
) -> Json<serde_json::Value> {
    let limit = params.limit.unwrap_or(10);

    let results = if let Ok(index) = state.sparse_index.read() {
        let si: &SparseIndex = &index;
        match si.search(&params.q, limit) {
            Ok(results) => results,
            Err(e) => {
                return Json(serde_json::json!({
                    "success": false,
                    "error": e.to_string(),
                }));
            }
        }
    } else {
        return Json(serde_json::json!({
            "success": false,
            "error": "Index locked",
        }));
    };

    Json(serde_json::json!({
        "success": true,
        "query": params.q,
        "count": results.len(),
        "results": results.iter().map(|r| {
            serde_json::json!({
                "block_id": r.block_id,
                "url": r.url,
                "title": r.title,
                "snippet": r.text_snippet,
                "score": r.score,
                "block_type": r.block_type,
                "source_url": r.source_url,
            })
        }).collect::<Vec<_>>(),
    }))
}

// ─── Dense/Neural Index Search Handler ──────────────────────────────────────

#[derive(Debug, serde::Deserialize)]
pub struct NeuralIndexRequest {
    pub query: String,
    pub limit: Option<usize>,
}

async fn neural_index_handler(
    State(state): State<Arc<AppState>>,
    Json(req): Json<NeuralIndexRequest>,
) -> Json<serde_json::Value> {
    let limit = req.limit.unwrap_or(10);

    let results = if let Ok(index) = state.dense_index.read() {
        let di: &DenseIndex = &index;
        di.search(&req.query, limit)
    } else {
        return Json(serde_json::json!({
            "success": false,
            "error": "Dense index locked",
        }));
    };

    Json(serde_json::json!({
        "success": true,
        "query": req.query,
        "count": results.len(),
        "results": results.iter().map(|r| {
            serde_json::json!({
                "block_id": r.block_id,
                "url": r.url,
                "title": r.title,
                "snippet": r.text_snippet,
                "similarity": r.similarity,
                "block_type": r.block_type,
            })
        }).collect::<Vec<_>>(),
    }))
}

// ─── Hybrid Index Search Handler (BM25 + Dense RRF Fusion) ─────────────────

#[derive(Debug, serde::Deserialize)]
pub struct HybridIndexRequest {
    pub query: String,
    pub limit: Option<usize>,
    /// Search mode: "hybrid" (default), "bm25_only", "dense_only"
    pub mode: Option<String>,
    /// RRF constant k (default 60)
    pub rrf_k: Option<f64>,
    /// BM25 weight (default 1.0)
    pub bm25_weight: Option<f64>,
    /// Dense weight (default 1.0)
    pub dense_weight: Option<f64>,
}

async fn hybrid_index_handler(
    State(state): State<Arc<AppState>>,
    Json(req): Json<HybridIndexRequest>,
) -> Json<serde_json::Value> {
    let limit = req.limit.unwrap_or(20);
    let mode = req.mode.as_deref().unwrap_or("hybrid");

    let rrf_config = RrfConfig {
        k: req.rrf_k.unwrap_or(60.0),
        bm25_weight: req.bm25_weight.unwrap_or(1.0),
        dense_weight: req.dense_weight.unwrap_or(1.0),
        limit,
    };

    // Build hybrid searcher from current indexes
    {
        let sparse = state.sparse_index.read();
        let dense = state.dense_index.read();

        match (sparse, dense) {
            (Ok(si), Ok(di)) => {
                // Clone indexes for the searcher (they're Arc-backed internally)
                // For now, use the search methods directly
                let bm25_results = si.search(&req.query, limit).unwrap_or_default();
                let dense_results = di.search(&req.query, limit);

                match mode {
                    "bm25_only" => {
                        let results = bm25_results
                            .into_iter()
                            .enumerate()
                            .map(|(rank, r)| {
                                serde_json::json!({
                                    "block_id": r.block_id,
                                    "url": r.url,
                                    "title": r.title,
                                    "snippet": r.text_snippet,
                                    "hybrid_score": r.score,
                                    "bm25_score": r.score,
                                    "bm25_rank": rank + 1,
                                    "dense_similarity": null,
                                    "dense_rank": null,
                                    "block_type": r.block_type,
                                })
                            })
                            .collect::<Vec<_>>();

                        return Json(serde_json::json!({
                            "success": true,
                            "mode": "bm25_only",
                            "query": req.query,
                            "count": results.len(),
                            "results": results,
                        }));
                    }
                    "dense_only" => {
                        let results = dense_results
                            .into_iter()
                            .enumerate()
                            .map(|(rank, r)| {
                                serde_json::json!({
                                    "block_id": r.block_id,
                                    "url": r.url,
                                    "title": r.title,
                                    "snippet": r.text_snippet,
                                    "hybrid_score": r.similarity,
                                    "bm25_score": null,
                                    "bm25_rank": null,
                                    "dense_similarity": r.similarity,
                                    "dense_rank": rank + 1,
                                    "block_type": r.block_type,
                                })
                            })
                            .collect::<Vec<_>>();

                        return Json(serde_json::json!({
                            "success": true,
                            "mode": "dense_only",
                            "query": req.query,
                            "count": results.len(),
                            "results": results,
                        }));
                    }
                    _ => {
                        // Hybrid RRF fusion
                        let fused = markify_core::index::hybrid::reciprocal_rank_fusion(
                            bm25_results,
                            dense_results,
                            &rrf_config,
                        );

                        let results = fused
                            .into_iter()
                            .map(|r| {
                                serde_json::json!({
                                    "block_id": r.block_id,
                                    "url": r.url,
                                    "title": r.title,
                                    "snippet": r.text_snippet,
                                    "hybrid_score": r.hybrid_score,
                                    "bm25_score": r.bm25_score,
                                    "bm25_rank": r.bm25_rank,
                                    "dense_similarity": r.dense_similarity,
                                    "dense_rank": r.dense_rank,
                                    "block_type": r.block_type,
                                })
                            })
                            .collect::<Vec<_>>();

                        return Json(serde_json::json!({
                            "success": true,
                            "mode": "hybrid_rrf",
                            "rrf_k": rrf_config.k,
                            "bm25_weight": rrf_config.bm25_weight,
                            "dense_weight": rrf_config.dense_weight,
                            "query": req.query,
                            "count": results.len(),
                            "results": results,
                        }));
                    }
                }
            }
            _ => {
                return Json(serde_json::json!({
                    "success": false,
                    "error": "Index locked",
                }));
            }
        }
    };

    (); // suppress unused variable warning
}

// ─── Crawl Engine Handlers ───────────────────────────────────────────────────

use markify_core::crawl::engine::CrawlJob as EngineCrawlJob;

#[derive(Debug, serde::Deserialize)]
pub struct CrawlStartRequest {
    /// Seed URL
    pub url: String,
    /// Job name
    pub name: Option<String>,
    /// Max pages to crawl
    pub max_pages: Option<usize>,
    /// Max crawl depth
    pub max_depth: Option<usize>,
    /// Include patterns (regex)
    pub include_patterns: Option<Vec<String>>,
    /// Exclude patterns (regex)
    pub exclude_patterns: Option<Vec<String>>,
    /// Respect robots.txt
    pub respect_robots_txt: Option<bool>,
}

async fn crawl_start_handler(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CrawlStartRequest>,
) -> Json<serde_json::Value> {
    if req.url.is_empty() {
        return Json(serde_json::json!({
            "success": false,
            "error": "Seed URL cannot be empty",
        }));
    }

    let job_id = format!("crawl-{}", uuid::Uuid::new_v4());
    let max_depth = req.max_depth.unwrap_or(3);

    let mut job = EngineCrawlJob::new(
        &job_id,
        req.name
            .as_deref()
            .unwrap_or(&format!("Crawl: {}", req.url)),
        vec![req.url.clone()],
        max_depth,
    );

    if let Some(max_pages) = req.max_pages {
        job.max_pages = max_pages;
    }
    if let Some(patterns) = req.include_patterns {
        job.include_patterns = patterns;
    }
    if let Some(patterns) = req.exclude_patterns {
        job.exclude_patterns = patterns;
    }
    if let Some(respect) = req.respect_robots_txt {
        job.respect_robots_txt = respect;
    }

    // Add seed URLs to frontier
    let seed_url = FrontierUrl::new(&req.url, &job_id, max_depth);
    if let Ok(mut frontier) = state.crawl_frontier.write() {
        frontier.push(seed_url);
    }

    // Add to bloom filter
    if let Ok(mut bloom) = state.crawl_bloom.write() {
        bloom.add(&req.url);
    }

    // Store job
    let legacy_job = CrawlJob {
        id: job_id.clone(),
        status: CrawlStatus::Running,
        seed_url: req.url.clone(),
        pages_crawled: 0,
        pages_queued: 1,
        pages_failed: 0,
        started_at: Some(chrono::Utc::now()),
        completed_at: None,
    };

    if let Ok(mut jobs) = state.crawl_jobs.write() {
        jobs.insert(job_id.clone(), legacy_job);
    }

    Json(serde_json::json!({
        "success": true,
        "job_id": job_id,
        "seed_url": req.url,
        "max_depth": max_depth,
        "max_pages": job.max_pages,
        "status": "running",
        "message": "Crawl job started successfully",
    }))
}

async fn crawl_status_handler(
    State(state): State<Arc<AppState>>,
    axum::extract::Query(params): axum::extract::Query<CrawlStatusQuery>,
) -> Json<serde_json::Value> {
    let jobs = state.crawl_jobs.read();

    match jobs {
        Ok(jobs) => {
            if let Some(job_id) = params.job_id {
                // Get specific job
                if let Some(job) = jobs.get(&job_id) {
                    let frontier_stats = state.crawl_frontier.read().ok().map(|f| f.stats());
                    return Json(serde_json::json!({
                        "success": true,
                        "job": job,
                        "frontier": frontier_stats,
                    }));
                }
                return Json(serde_json::json!({
                    "success": false,
                    "error": format!("Job {} not found", job_id),
                }));
            }

            // List all jobs
            let job_list: Vec<_> = jobs.values().collect();
            Json(serde_json::json!({
                "success": true,
                "count": job_list.len(),
                "jobs": job_list,
            }))
        }
        Err(_) => Json(serde_json::json!({
            "success": false,
            "error": "Jobs locked",
        })),
    }
}

#[derive(Debug, serde::Deserialize)]
pub struct CrawlStatusQuery {
    pub job_id: Option<String>,
}

async fn crawl_stop_handler(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CrawlStopRequest>,
) -> Json<serde_json::Value> {
    if let Ok(mut jobs) = state.crawl_jobs.write() {
        if let Some(job) = jobs.get_mut(&req.job_id) {
            job.status = CrawlStatus::Cancelled;
            job.completed_at = Some(chrono::Utc::now());
            return Json(serde_json::json!({
                "success": true,
                "job_id": req.job_id,
                "status": "cancelled",
            }));
        }
    }

    Json(serde_json::json!({
        "success": false,
        "error": format!("Job {} not found", req.job_id),
    }))
}

#[derive(Debug, serde::Deserialize)]
pub struct CrawlStopRequest {
    pub job_id: String,
}

async fn crawl_list_jobs_handler(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let job_list = state
        .crawl_jobs
        .read()
        .map(|j| j.values().cloned().collect::<Vec<_>>())
        .unwrap_or_default();

    let frontier_stats = state.crawl_frontier.read().ok().map(|f| {
        let stats = f.stats();
        serde_json::json!({
            "total": stats.total,
            "critical": stats.critical,
            "high": stats.high,
            "normal": stats.normal,
            "low": stats.low,
            "domain_count": stats.domain_count,
        })
    });

    let bloom_count = state.crawl_bloom.read().ok().map(|b| b.len());

    Json(serde_json::json!({
        "success": true,
        "total_jobs": job_list.len(),
        "jobs": job_list,
        "frontier": frontier_stats,
        "bloom_filter_size": bloom_count,
    }))
}

async fn crawl_results_handler(
    State(state): State<Arc<AppState>>,
    axum::extract::Query(params): axum::extract::Query<CrawlResultsQuery>,
) -> Json<serde_json::Value> {
    if let Some(job_id) = params.job_id {
        let results = state.crawl_results.read();
        if let Ok(results) = results {
            if let Some(job_results) = results.get(&job_id) {
                return Json(serde_json::json!({
                    "success": true,
                    "job_id": job_id,
                    "count": job_results.len(),
                    "results": job_results,
                }));
            }
        }
        return Json(serde_json::json!({
            "success": false,
            "error": format!("No results found for job {}", job_id),
        }));
    }

    Json(serde_json::json!({
        "success": false,
        "error": "job_id query parameter is required",
    }))
}

#[derive(Debug, serde::Deserialize)]
pub struct CrawlResultsQuery {
    pub job_id: Option<String>,
}
