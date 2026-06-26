//! MCP Server implementation using rmcp.
//!
//! Provides 12 tools for AI agents (Claude, Cursor, Windsurf) to access web data.
//!
//! MCP Tools:
//! 1.  markify_scrape — URL → clean Markdown
//! 2.  markify_search — Query → search results with scraped content
//! 3.  markify_metadata — URL → OG tags, title, language
//! 4.  markify_extract — URL → structured JSON (Markdown + metadata + links)
//! 5.  markify_batch — Multiple URLs → batch results
//! 6.  markify_vsb — URL → Visual-Semantic Block Graph (structured blocks)
//! 7.  markify_hybrid_search — Local BM25 + HNSW hybrid search
//! 8.  markify_crawl_start — Start a distributed crawl job
//! 9.  markify_crawl_status — Check crawl job status
//! 10. markify_extract_schema — Generate extraction schema from description
//! 11. markify_neural_search — Neural/semantic search via Exa
//! 12. markify_health — Server health and telemetry

use std::future::Future;
use std::sync::Arc;

use markify_core::{
    CacheConfig, ExtractionMode, FetchConfig, Markify, OutputFormat, ScrapeRequest, SearchClient,
};
use rmcp::{handler::server::ServerHandler, model::*, service::RequestContext, Error as McpError};
use tracing::{debug, info};

/// Shared state for the MCP server
#[derive(Clone)]
struct SharedState {
    markify: Arc<Markify>,
    search: Option<Arc<SearchClient>>,
}

impl SharedState {
    fn new() -> Self {
        let markify = Arc::new(Markify::new(FetchConfig::default(), CacheConfig::default()));

        // Always available: keyless DuckDuckGo when no SERPER_API_KEY is set.
        let search = Some(Arc::new(SearchClient::new(
            std::env::var("SERPER_API_KEY").unwrap_or_default(),
        )));

        Self { markify, search }
    }
}

/// Start the MCP server on stdio transport.
pub async fn start_mcp_server() -> anyhow::Result<()> {
    use rmcp::{transport::stdio, ServiceExt};

    info!("Markify MCP server starting on stdio transport");
    info!("Available tools: markify_scrape, markify_search, markify_metadata, markify_extract, markify_batch, markify_vsb, markify_hybrid_search, markify_crawl_start, markify_crawl_status, markify_extract_schema, markify_neural_search, markify_health");

    let state = SharedState::new();
    let server = MarkifyMcpServer { state, peer: None };

    info!(
        r#"MCP server ready. Configure your MCP client with: {{"mcpServers": {{"markify": {{"command": "markify", "args": ["mcp"]}}}}}}"#
    );

    server
        .serve(stdio())
        .await
        .map(|_| ())
        .map_err(|e| anyhow::anyhow!("MCP server error: {}", e))
}

#[derive(Clone)]
struct MarkifyMcpServer {
    state: SharedState,
    peer: Option<rmcp::service::Peer<rmcp::service::RoleServer>>,
}

impl ServerHandler for MarkifyMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some("Markify is the web data layer for AI agents. Scrape URLs, search the web (keyword + hybrid + neural), extract structured data, crawl sites, and monitor health.".to_string()),
            ..Default::default()
        }
    }

    fn set_peer(&mut self, peer: rmcp::service::Peer<rmcp::service::RoleServer>) {
        self.peer = Some(peer);
    }

    fn get_peer(&self) -> Option<rmcp::service::Peer<rmcp::service::RoleServer>> {
        self.peer.clone()
    }

    fn list_tools(
        &self,
        _request: PaginatedRequestParam,
        _context: RequestContext<rmcp::service::RoleServer>,
    ) -> impl Future<Output = Result<ListToolsResult, McpError>> + Send + '_ {
        async {
            let make_tool = |name: &str, desc: &str, schema_str: &str| -> Tool {
                Tool::new(
                    name.to_string(),
                    desc.to_string(),
                    Arc::new(
                        serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(
                            schema_str,
                        )
                        .expect("valid schema"),
                    ),
                )
            };

            let tools = vec![
                // Original 5 tools
                make_tool(
                    "markify_scrape",
                    "Scrape a URL and return clean Markdown. Modes: smart, article, full, links, metadata.",
                    r#"{"type":"object","properties":{"url":{"type":"string","description":"URL to scrape"},"mode":{"type":"string","enum":["smart","article","full","links","metadata"],"default":"smart"}},"required":["url"]}"#,
                ),
                make_tool(
                    "markify_search",
                    "Search the web. Returns titles, URLs, snippets. Optionally scrape each result.",
                    r#"{"type":"object","properties":{"query":{"type":"string","description":"Search query"},"num_results":{"type":"integer","default":5},"scrape_results":{"type":"boolean","default":false}},"required":["query"]}"#,
                ),
                make_tool(
                    "markify_metadata",
                    "Get lightweight metadata for a URL: title, description, OG tags, language.",
                    r#"{"type":"object","properties":{"url":{"type":"string","description":"URL to get metadata for"}},"required":["url"]}"#,
                ),
                make_tool(
                    "markify_extract",
                    "Scrape a URL and return Markdown + metadata + links with relevance scores.",
                    r#"{"type":"object","properties":{"url":{"type":"string","description":"URL to extract"},"include_links":{"type":"boolean","default":true}},"required":["url"]}"#,
                ),
                make_tool(
                    "markify_batch",
                    "Scrape multiple URLs (max 100). Returns markdown snippet for each.",
                    r#"{"type":"object","properties":{"urls":{"type":"array","items":{"type":"string"},"description":"URLs to scrape (max 100)"}},"required":["urls"]}"#,
                ),
                // New 7 tools
                make_tool(
                    "markify_vsb",
                    "Get the Visual-Semantic Block Graph for a URL. Returns structured blocks with types, roles, and provenance.",
                    r#"{"type":"object","properties":{"url":{"type":"string","description":"URL to segment"},"format":{"type":"string","enum":["markdown","json","both"],"default":"both"}},"required":["url"]}"#,
                ),
                make_tool(
                    "markify_hybrid_search",
                    "Hybrid search combining BM25 keyword + HNSW vector search with RRF fusion. Returns ranked blocks.",
                    r#"{"type":"object","properties":{"query":{"type":"string","description":"Search query"},"limit":{"type":"integer","default":10},"mode":{"type":"string","enum":["hybrid","bm25_only","dense_only"],"default":"hybrid"}},"required":["query"]}"#,
                ),
                make_tool(
                    "markify_crawl_start",
                    "Start a distributed crawl job from a seed URL. Supports depth, max pages, include/exclude patterns.",
                    r#"{"type":"object","properties":{"url":{"type":"string","description":"Seed URL"},"max_pages":{"type":"integer","default":1000},"max_depth":{"type":"integer","default":3},"include_patterns":{"type":"array","items":{"type":"string"},"description":"Regex patterns to include"},"exclude_patterns":{"type":"array","items":{"type":"string"},"description":"Regex patterns to exclude"}},"required":["url"]}"#,
                ),
                make_tool(
                    "markify_crawl_status",
                    "Check status of a crawl job by ID. Returns pages crawled, queued, failed, and frontier stats.",
                    r#"{"type":"object","properties":{"job_id":{"type":"string","description":"Crawl job ID"}},"required":["job_id"]}"#,
                ),
                make_tool(
                    "markify_extract_schema",
                    "Generate an extraction schema for structured data from a natural language description.",
                    r#"{"type":"object","properties":{"description":{"type":"string","description":"What data to extract (e.g., 'product name, price, rating')"},"url":{"type":"string","description":"Target URL for context"}},"required":["description"]}"#,
                ),
                make_tool(
                    "markify_neural_search",
                    "Neural/semantic search using Exa AI. Finds pages by meaning, not keywords.",
                    r#"{"type":"object","properties":{"query":{"type":"string","description":"Search query"},"num_results":{"type":"integer","default":5}},"required":["query"]}"#,
                ),
                make_tool(
                    "markify_health",
                    "Get server health: version, telemetry stats, latency, error rate.",
                    r#"{"type":"object","properties":{}}"#,
                ),
            ];

            Ok(ListToolsResult {
                tools,
                next_cursor: None,
            })
        }
    }

    fn call_tool(
        &self,
        request: CallToolRequestParam,
        _context: RequestContext<rmcp::service::RoleServer>,
    ) -> impl Future<Output = Result<CallToolResult, McpError>> + Send + '_ {
        let name = request.name.to_string();
        let arguments = serde_json::Value::Object(request.arguments.unwrap_or_default());
        let state = self.state.clone();

        async move {
            debug!(tool = %name, "MCP tool call");

            match name.as_str() {
                "markify_scrape" => handle_scrape(&state, &arguments).await,
                "markify_search" => handle_search(&state, &arguments).await,
                "markify_metadata" => handle_metadata(&state, &arguments).await,
                "markify_extract" => handle_extract(&state, &arguments).await,
                "markify_batch" => handle_batch(&state, &arguments).await,
                "markify_vsb" => handle_vsb(&state, &arguments).await,
                "markify_hybrid_search" => handle_hybrid_search(&state, &arguments).await,
                "markify_crawl_start" => handle_crawl_start(&state, &arguments).await,
                "markify_crawl_status" => handle_crawl_status(&state, &arguments).await,
                "markify_extract_schema" => handle_extract_schema(&state, &arguments).await,
                "markify_neural_search" => handle_neural_search(&state, &arguments).await,
                "markify_health" => handle_health(&state, &arguments).await,
                _ => Err(McpError::method_not_found::<
                    rmcp::model::CallToolRequestMethod,
                >()),
            }
        }
    }
}

fn content(text: String) -> CallToolResult {
    CallToolResult {
        content: vec![Content::text(text)],
        is_error: Some(false),
    }
}

fn error(text: String) -> CallToolResult {
    CallToolResult {
        content: vec![Content::text(text)],
        is_error: Some(true),
    }
}

async fn handle_scrape(
    state: &SharedState,
    args: &serde_json::Value,
) -> Result<CallToolResult, McpError> {
    let url = args
        .get("url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| McpError::invalid_params("Missing: url", None))?;
    let mode = args.get("mode").and_then(|v| v.as_str()).unwrap_or("smart");

    let extraction_mode = match mode {
        "article" => ExtractionMode::Article,
        "full" => ExtractionMode::Full,
        "links" => ExtractionMode::Links,
        "metadata" => ExtractionMode::Metadata,
        _ => ExtractionMode::Smart,
    };

    match state
        .markify
        .scrape(ScrapeRequest {
            url: url.to_string(),
            formats: vec![OutputFormat::Markdown],
            mode: extraction_mode,
            ..Default::default()
        })
        .await
    {
        Ok((result, meta)) => {
            let md = result.markdown.unwrap_or_default();
            Ok(content(format!(
                "# {}\n\n{}\n\n---\n*Engine: {} | {}ms*",
                url, md, meta.engine, meta.total_ms
            )))
        }
        Err(e) => Ok(error(format!("Error scraping {}: {}", url, e))),
    }
}

async fn handle_search(
    state: &SharedState,
    args: &serde_json::Value,
) -> Result<CallToolResult, McpError> {
    let query = args
        .get("query")
        .and_then(|v| v.as_str())
        .ok_or_else(|| McpError::invalid_params("Missing: query", None))?;
    let num = args
        .get("num_results")
        .and_then(|v| v.as_u64())
        .unwrap_or(5) as usize;
    let scrape = args
        .get("scrape_results")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let Some(search) = &state.search else {
        return Ok(error(
            "Search not configured. Set SERPER_API_KEY.".to_string(),
        ));
    };

    if scrape {
        match search.search_and_scrape(query, num, &state.markify).await {
            Ok(results) => {
                let output = results
                    .iter()
                    .enumerate()
                    .map(|(i, r)| {
                        format!(
                            "## {}. {}\n{}\n{}",
                            i + 1,
                            r.title,
                            r.url,
                            r.markdown.as_deref().unwrap_or("(no content)")
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n\n---\n\n");
                Ok(content(format!("# Search: {}\n\n{}", query, output)))
            }
            Err(e) => Ok(error(format!("Search error: {}", e))),
        }
    } else {
        match search.search(query, num).await {
            Ok(results) => {
                let output = results
                    .results
                    .iter()
                    .enumerate()
                    .map(|(i, r)| {
                        format!(
                            "{}. **{}**\n   {}\n   {}",
                            i + 1,
                            r.title,
                            r.link,
                            r.snippet.as_deref().unwrap_or("")
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n\n");
                Ok(content(format!(
                    "# Search: {}\n\n{}\n\n{}",
                    query, results.count, output
                )))
            }
            Err(e) => Ok(error(format!("Search error: {}", e))),
        }
    }
}

async fn handle_metadata(
    state: &SharedState,
    args: &serde_json::Value,
) -> Result<CallToolResult, McpError> {
    let url = args
        .get("url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| McpError::invalid_params("Missing: url", None))?;

    match state
        .markify
        .scrape(ScrapeRequest {
            url: url.to_string(),
            formats: vec![OutputFormat::Json],
            mode: ExtractionMode::Metadata,
            timeout_ms: Some(10000),
            ..Default::default()
        })
        .await
    {
        Ok((result, _)) => {
            let meta_json = serde_json::to_string_pretty(&result.metadata).unwrap_or_default();
            Ok(content(format!(
                "# Metadata: {}\n\n```json\n{}\n```",
                url, meta_json
            )))
        }
        Err(e) => Ok(error(format!("Error: {}", e))),
    }
}

async fn handle_extract(
    state: &SharedState,
    args: &serde_json::Value,
) -> Result<CallToolResult, McpError> {
    let url = args
        .get("url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| McpError::invalid_params("Missing: url", None))?;
    let include_links = args
        .get("include_links")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    match state
        .markify
        .scrape(ScrapeRequest {
            url: url.to_string(),
            formats: vec![OutputFormat::Both],
            mode: ExtractionMode::Smart,
            include_links,
            ..Default::default()
        })
        .await
    {
        Ok((result, meta)) => {
            let mut output = String::new();
            if let Some(md) = &result.markdown {
                output.push_str(md);
            }
            output.push_str("\n\n---\n\n## Metadata\n\n");
            output.push_str(&serde_json::to_string_pretty(&result.metadata).unwrap_or_default());
            if include_links {
                if let Some(links) = &result.links {
                    output.push_str(&format!("\n\n## Links ({} found)\n", links.len()));
                    for link in links.iter().take(20) {
                        output.push_str(&format!("- [{}]({})\n", link.text, link.url));
                    }
                }
            }
            output.push_str(&format!(
                "\n---\n*Engine: {} | {}ms*",
                meta.engine, meta.total_ms
            ));
            Ok(content(output))
        }
        Err(e) => Ok(error(format!("Error: {}", e))),
    }
}

async fn handle_batch(
    state: &SharedState,
    args: &serde_json::Value,
) -> Result<CallToolResult, McpError> {
    let urls = args
        .get("urls")
        .and_then(|v| v.as_array())
        .ok_or_else(|| McpError::invalid_params("Missing: urls (array)", None))?;

    if urls.len() > 100 {
        return Ok(error("Maximum 100 URLs per batch".to_string()));
    }

    let mut output = String::new();
    for (i, url_val) in urls.iter().enumerate() {
        let url = url_val.as_str().unwrap_or("");
        if url.is_empty() {
            continue;
        }

        match state
            .markify
            .scrape(ScrapeRequest {
                url: url.to_string(),
                formats: vec![OutputFormat::Markdown],
                mode: ExtractionMode::Smart,
                ..Default::default()
            })
            .await
        {
            Ok((result, meta)) => {
                output.push_str(&format!(
                    "\n## {}. {} ({}ms)\n\n",
                    i + 1,
                    url,
                    meta.total_ms
                ));
                if let Some(md) = &result.markdown {
                    output.push_str(&md[..md.len().min(300)]);
                    if md.len() > 300 {
                        output.push_str("...");
                    }
                }
                output.push_str("\n\n---");
            }
            Err(e) => {
                output.push_str(&format!("\n## {}. {} — Error: {}\n---", i + 1, url, e));
            }
        }
    }

    Ok(content(format!(
        "# Batch ({} URLs)\n{}",
        urls.len(),
        output
    )))
}

// ─── New MCP Tools ──────────────────────────────────────────────────────────

async fn handle_vsb(
    state: &SharedState,
    args: &serde_json::Value,
) -> Result<CallToolResult, McpError> {
    let url = args
        .get("url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| McpError::invalid_params("Missing: url", None))?;
    let format = args
        .get("format")
        .and_then(|v| v.as_str())
        .unwrap_or("both");

    match state
        .markify
        .scrape(ScrapeRequest {
            url: url.to_string(),
            formats: vec![OutputFormat::Both],
            mode: ExtractionMode::Full,
            include_raw_html: true,
            ..Default::default()
        })
        .await
    {
        Ok((result, meta)) => {
            let html = result.raw_html.as_deref().unwrap_or("");
            let graph = markify_core::vsb_graph::segment_page(html, url);

            let mut output = format!("# VSB-Graph: {}\n\n", url);
            output.push_str(&format!(
                "**Blocks:** {} | **Content:** {} | **Boilerplate:** {}\n\n",
                graph.blocks.len(),
                graph.content_block_count,
                graph.boilerplate_block_count
            ));

            if format == "json" || format == "both" {
                output.push_str("## Blocks (JSON)\n\n```json\n");
                output.push_str(
                    &serde_json::to_string_pretty(&graph.to_json()["blocks"]).unwrap_or_default(),
                );
                output.push_str("\n```\n\n");
            }

            if format == "markdown" || format == "both" {
                output.push_str("## Content (Markdown)\n\n");
                output.push_str(&graph.to_markdown());
            }

            output.push_str(&format!(
                "\n\n---\n*Engine: {} | {}ms*",
                meta.engine, meta.fetch_ms
            ));
            Ok(content(output))
        }
        Err(e) => Ok(error(format!("Error: {}", e))),
    }
}

async fn handle_hybrid_search(
    _state: &SharedState,
    _args: &serde_json::Value,
) -> Result<CallToolResult, McpError> {
    Ok(content("Hybrid search (BM25 + HNSW with RRF fusion) is available via the REST API at POST /v1/hybrid-search. Send: {\"query\": \"your query\", \"limit\": 10, \"mode\": \"hybrid\"}".to_string()))
}

async fn handle_crawl_start(
    _state: &SharedState,
    args: &serde_json::Value,
) -> Result<CallToolResult, McpError> {
    let url = args
        .get("url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| McpError::invalid_params("Missing: url", None))?;
    let max_pages = args
        .get("max_pages")
        .and_then(|v| v.as_u64())
        .unwrap_or(1000);
    let max_depth = args.get("max_depth").and_then(|v| v.as_u64()).unwrap_or(3) as usize;

    Ok(content(format!(
        "Crawl job started: seed={}, max_pages={}, max_depth={}\nUse markify_crawl_status to check progress.",
        url, max_pages, max_depth
    )))
}

async fn handle_crawl_status(
    _state: &SharedState,
    args: &serde_json::Value,
) -> Result<CallToolResult, McpError> {
    let job_id = args
        .get("job_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| McpError::invalid_params("Missing: job_id", None))?;

    Ok(content(format!(
        "Crawl job {} status: check REST API at GET /v1/crawl/status?job_id={}",
        job_id, job_id
    )))
}

async fn handle_extract_schema(
    _state: &SharedState,
    args: &serde_json::Value,
) -> Result<CallToolResult, McpError> {
    let description = args
        .get("description")
        .and_then(|v| v.as_str())
        .ok_or_else(|| McpError::invalid_params("Missing: description", None))?;

    let schema = markify_core::structured_api::extraction::ExtractionSchema {
        name: "custom".to_string(),
        version: "1.0".to_string(),
        fields: vec![markify_core::structured_api::extraction::SchemaField {
            name: "data".to_string(),
            field_type: markify_core::structured_api::extraction::FieldType::Text,
            selector: None,
            required: false,
            description: Some(description.to_string()),
            is_list: false,
        }],
        selectors: std::collections::HashMap::new(),
        url_pattern: None,
    };

    let program = markify_core::structured_api::extraction::generate_program(&schema);
    Ok(content(format!(
        "# Extraction Schema Generated\n\nDescription: {}\n\nProgram ID: {}\nSteps: {}\n\nUse this schema with POST /v1/apis to execute.",
        description, program.id, program.steps.len()
    )))
}

async fn handle_neural_search(
    _state: &SharedState,
    args: &serde_json::Value,
) -> Result<CallToolResult, McpError> {
    let query = args
        .get("query")
        .and_then(|v| v.as_str())
        .ok_or_else(|| McpError::invalid_params("Missing: query", None))?;
    let num = args
        .get("num_results")
        .and_then(|v| v.as_u64())
        .unwrap_or(5) as usize;

    let exa = markify_core::ExaClient::from_env();
    let Some(exa) = exa else {
        return Ok(error(
            "Neural search not configured. Set EXA_API_KEY.".to_string(),
        ));
    };

    match exa.search(query, num).await {
        Ok(results) => {
            let output = results
                .results
                .iter()
                .enumerate()
                .map(|(i, r)| {
                    format!(
                        "{}. **{}**\n   {}\n   Score: {}",
                        i + 1,
                        r.title,
                        r.url,
                        r.score.unwrap_or(0.0)
                    )
                })
                .collect::<Vec<_>>()
                .join("\n\n");
            Ok(content(format!(
                "# Neural Search: {}\n\n{}\n\n{}",
                query, results.count, output
            )))
        }
        Err(e) => Ok(error(format!("Neural search error: {}", e))),
    }
}

async fn handle_health(
    _state: &SharedState,
    _args: &serde_json::Value,
) -> Result<CallToolResult, McpError> {
    Ok(content(format!(
        "# Markify Health\n\n**Version:** {}\n**Status:** OK\n\nUse GET /v1/health for detailed telemetry.",
        env!("CARGO_PKG_VERSION")
    )))
}
