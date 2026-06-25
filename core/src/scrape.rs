//! Main scraping interface. Ties together fetching, extraction, and transformation.

use serde::{Deserialize, Serialize};
use std::time::Instant;
use tracing::{debug, info};

use crate::cache::{CacheConfig, MarkifyCache};
use crate::extract::{
    links::extract_links, metadata::extract_metadata, readability::extract_article, ExtractionMode,
    LinkInfo, Metadata,
};
use crate::fetch::{router::FetchEngine, FetchConfig, FetchRouter};
use crate::transform::{json::to_structured_json, markdown::to_markdown, OutputFormat};

/// Request to scrape a URL
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScrapeRequest {
    /// URL to scrape
    pub url: String,
    /// Output formats
    #[serde(default)]
    pub formats: Vec<OutputFormat>,
    /// Extraction mode
    #[serde(default)]
    pub mode: ExtractionMode,
    /// CSS selector to wait for (browser mode)
    pub wait_for_selector: Option<String>,
    /// Timeout in milliseconds
    pub timeout_ms: Option<u64>,
    /// Force browser rendering
    #[serde(default)]
    pub force_browser: bool,
    /// Include raw HTML in response
    #[serde(default)]
    pub include_raw_html: bool,
    /// Include links in response
    #[serde(default)]
    pub include_links: bool,
    /// Include images in response
    #[serde(default)]
    pub include_images: bool,
    /// User-defined JSON schema for AI extraction
    pub extract_schema: Option<serde_json::Value>,
}

impl Default for ScrapeRequest {
    fn default() -> Self {
        Self {
            url: String::new(),
            formats: vec![OutputFormat::Both],
            mode: ExtractionMode::Smart,
            wait_for_selector: None,
            timeout_ms: None,
            force_browser: false,
            include_raw_html: false,
            include_links: false,
            include_images: false,
            extract_schema: None,
        }
    }
}

/// Result of a scrape operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScrapeResult {
    /// The original URL
    pub url: String,
    /// Final URL after redirects
    pub final_url: String,
    /// HTTP status code
    pub status_code: u16,
    /// Whether the request succeeded
    pub success: bool,
    /// Markdown output
    pub markdown: Option<String>,
    /// Structured JSON output
    pub json_content: Option<serde_json::Value>,
    /// Extracted content
    pub extracted: Option<serde_json::Value>,
    /// Page metadata
    pub metadata: Option<Metadata>,
    /// Links (if requested)
    pub links: Option<Vec<LinkInfo>>,
    /// Raw HTML (if requested)
    pub raw_html: Option<String>,
    /// Error message (if failed)
    pub error: Option<String>,
}

/// Performance metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScrapeMeta {
    /// Was this served from cache?
    pub cached: bool,
    /// Which engine was used: "http" or "browser"
    pub engine: String,
    /// Fetch time in milliseconds
    pub fetch_ms: u64,
    /// Extract time in milliseconds
    pub extract_ms: u64,
    /// Total time in milliseconds
    pub total_ms: u64,
}

/// Main Markify client
pub struct Markify {
    fetch_router: FetchRouter,
    cache: MarkifyCache,
    fetch_config: FetchConfig,
}

impl Default for Markify {
    /// Zero-config client using default fetch + cache settings.
    fn default() -> Self {
        Self::new(FetchConfig::default(), CacheConfig::default())
    }
}

impl Markify {
    pub fn new(fetch_config: FetchConfig, cache_config: CacheConfig) -> Self {
        Self {
            fetch_router: FetchRouter::new(&fetch_config),
            cache: MarkifyCache::new(cache_config),
            fetch_config,
        }
    }

    /// Scrape a single URL
    pub async fn scrape(
        &self,
        request: ScrapeRequest,
    ) -> anyhow::Result<(ScrapeResult, ScrapeMeta)> {
        let total_start = Instant::now();

        // Check cache
        let cache_key = MarkifyCache::make_key(&request.url, "scrape", "default");
        if let Some(cached) = self.cache.get(&cache_key).await {
            debug!(url = %request.url, "Cache hit");
            let cached_result: ScrapeResult = serde_json::from_slice(&cached.data)?;
            return Ok((
                cached_result,
                ScrapeMeta {
                    cached: true,
                    engine: "http".to_string(),
                    fetch_ms: 0,
                    extract_ms: 0,
                    total_ms: total_start.elapsed().as_millis() as u64,
                },
            ));
        }

        // Fetch
        let fetch_start = Instant::now();
        let timeout_ms = request
            .timeout_ms
            .unwrap_or(self.fetch_config.timeout_secs * 1000);
        let (page, engine) = self
            .fetch_router
            .fetch(
                &request.url,
                request.wait_for_selector.as_deref(),
                timeout_ms,
                request.force_browser,
            )
            .await?;
        let fetch_ms = fetch_start.elapsed().as_millis() as u64;

        // Extract
        let extract_start = Instant::now();
        let html = &page.html;
        let base_url: Option<&str> = Some(&page.url);

        // Extract metadata (always)
        let metadata = extract_metadata(html, base_url);

        // Extract content based on mode
        let (markdown, content_structured) = match &request.mode {
            ExtractionMode::Article => {
                let article = extract_article(html);
                let md = article
                    .as_ref()
                    .and_then(|a| a.content.clone())
                    .unwrap_or_else(|| to_markdown(html));
                (
                    Some(md),
                    article.as_ref().and_then(|a| a.structured.clone()),
                )
            }
            ExtractionMode::Full => {
                let md = to_markdown(html);
                (Some(md), None)
            }
            ExtractionMode::Metadata => (None, None),
            ExtractionMode::Links => {
                let links = extract_links(html, base_url);
                let md = links
                    .iter()
                    .map(|l| format!("[{}]({})", l.text, l.url))
                    .collect::<Vec<_>>()
                    .join("\n");
                (Some(md), Some(serde_json::json!({"links": links})))
            }
            ExtractionMode::Smart => {
                // Try article extraction first, fall back to full
                if let Some(article) = extract_article(html) {
                    let md = article.content.unwrap_or_else(|| to_markdown(html));
                    (Some(md), article.structured)
                } else {
                    (Some(to_markdown(html)), None)
                }
            }
            ExtractionMode::Images => {
                // Images are handled separately in the response
                (Some(to_markdown(html)), None)
            }
        };
        let extract_ms = extract_start.elapsed().as_millis() as u64;

        // Extract links if requested
        let links = if request.include_links {
            Some(extract_links(html, base_url))
        } else {
            None
        };

        // Build structured JSON
        let json_content = if request.formats.contains(&OutputFormat::Json)
            || request.formats.contains(&OutputFormat::Both)
        {
            let json = to_structured_json(
                metadata.title.clone(),
                markdown.clone(),
                Some(metadata.clone()),
                links.clone(),
                None,
            );
            Some(json)
        } else {
            None
        };

        let total_ms = total_start.elapsed().as_millis() as u64;

        let result = ScrapeResult {
            url: request.url.clone(),
            final_url: page.url.clone(),
            status_code: page.status_code,
            success: true,
            markdown: if request.formats.contains(&OutputFormat::Markdown)
                || request.formats.contains(&OutputFormat::Both)
            {
                markdown
            } else {
                None
            },
            json_content,
            extracted: content_structured,
            metadata: Some(metadata),
            links,
            raw_html: if request.include_raw_html {
                Some(page.html)
            } else {
                None
            },
            error: None,
        };

        // Cache the result
        if let Ok(data) = serde_json::to_vec(&result) {
            self.cache.insert(cache_key, data).await;
        }

        let meta = ScrapeMeta {
            cached: false,
            engine: match engine {
                FetchEngine::Http => "http".to_string(),
                FetchEngine::Browser => "browser".to_string(),
            },
            fetch_ms,
            extract_ms,
            total_ms,
        };

        info!(
            url = %request.url,
            status = page.status_code,
            engine = %meta.engine,
            total_ms = meta.total_ms,
            "Scrape complete"
        );

        Ok((result, meta))
    }

    /// Get cache statistics
    pub fn cache_stats(&self) -> String {
        self.cache.stats().to_string()
    }
}
