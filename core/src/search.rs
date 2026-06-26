//! Search module: web search via Serper API + query understanding + re-ranking.

pub mod query_understanding;
pub mod reranker;

pub use query_understanding::{
    extract_entities, rewrite_query, understand_query, IntentClassifier, QueryIntent,
    QueryRewriteResult, QueryUnderstandingResult, RewriteType,
};
pub use reranker::{CandidateDocument, CrossEncoderConfig, CrossEncoderReranker, ReRankedResult};

use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

/// Serper search result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    /// Search query
    pub query: String,
    /// Number of results
    pub count: usize,
    /// Organic search results
    pub results: Vec<SerperOrganicResult>,
}

/// Organic search result from Serper
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerperOrganicResult {
    pub title: String,
    pub link: String,
    pub snippet: Option<String>,
    pub position: Option<usize>,
}

/// Serper API response
#[derive(Debug, Deserialize)]
struct SerperResponse {
    #[serde(rename = "organic")]
    organic: Option<Vec<SerperOrganicResult>>,
    search_parameters: Option<SerperSearchParams>,
}

#[derive(Debug, Deserialize)]
struct SerperSearchParams {
    q: Option<String>,
}

/// Which backend a [`SearchClient`] uses.
#[derive(Debug, Clone)]
pub enum SearchBackend {
    /// Serper (Google results) — requires an API key.
    Serper(String),
    /// DuckDuckGo HTML — keyless, parsed from the public results page.
    DuckDuckGo,
}

/// Web search client. Uses Serper (Google) when a `SERPER_API_KEY` is
/// configured, otherwise falls back to **keyless** DuckDuckGo HTML search so
/// `/v1/search` works out of the box with no API key.
pub struct SearchClient {
    client: Client,
    backend: SearchBackend,
}

impl SearchClient {
    /// Build from a (possibly empty) Serper API key: a non-empty key uses
    /// Serper, an empty key falls back to keyless DuckDuckGo.
    pub fn new(api_key: String) -> Self {
        let backend = if api_key.trim().is_empty() {
            SearchBackend::DuckDuckGo
        } else {
            SearchBackend::Serper(api_key)
        };
        Self {
            client: Client::new(),
            backend,
        }
    }

    /// Keyless DuckDuckGo client.
    pub fn duckduckgo() -> Self {
        Self {
            client: Client::new(),
            backend: SearchBackend::DuckDuckGo,
        }
    }

    /// Name of the backend in use (for logs / health output).
    pub fn backend_name(&self) -> &'static str {
        match self.backend {
            SearchBackend::Serper(_) => "serper",
            SearchBackend::DuckDuckGo => "duckduckgo",
        }
    }

    /// Search the web and return organic results.
    pub async fn search(&self, query: &str, num_results: usize) -> anyhow::Result<SearchResult> {
        match &self.backend {
            SearchBackend::Serper(api_key) => self.search_serper(query, num_results, api_key).await,
            SearchBackend::DuckDuckGo => self.search_duckduckgo(query, num_results).await,
        }
    }

    async fn search_serper(
        &self,
        query: &str,
        num_results: usize,
        api_key: &str,
    ) -> anyhow::Result<SearchResult> {
        debug!(query = %query, count = num_results, "Searching via Serper");

        let response = self
            .client
            .post("https://google.serper.dev/search")
            .header("X-API-KEY", api_key)
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "q": query,
                "num": num_results,
            }))
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            warn!(status = %status, body = %body, "Serper API error");
            anyhow::bail!("Serper API error: {} - {}", status, body);
        }

        let serper_resp: SerperResponse = response.json().await?;

        let organic = serper_resp.organic.unwrap_or_default();
        let search_query = serper_resp
            .search_parameters
            .and_then(|sp| sp.q)
            .unwrap_or_else(|| query.to_string());

        debug!(results = organic.len(), query = %search_query, "Search complete");

        Ok(SearchResult {
            query: search_query,
            count: organic.len(),
            results: organic,
        })
    }

    /// Keyless search: fetch and parse DuckDuckGo's HTML results page.
    async fn search_duckduckgo(
        &self,
        query: &str,
        num_results: usize,
    ) -> anyhow::Result<SearchResult> {
        debug!(query = %query, count = num_results, "Searching via DuckDuckGo (keyless)");

        let response = self
            .client
            .get("https://html.duckduckgo.com/html/")
            .query(&[("q", query)])
            .header(
                "User-Agent",
                "Mozilla/5.0 (compatible; Markify/1.0; +https://github.com/skeehn/markify)",
            )
            .send()
            .await?;

        if !response.status().is_success() {
            anyhow::bail!("DuckDuckGo search error: {}", response.status());
        }

        let html = response.text().await?;
        let results = parse_ddg_results(&html, num_results);

        debug!(results = results.len(), query = %query, "Search complete");

        Ok(SearchResult {
            query: query.to_string(),
            count: results.len(),
            results,
        })
    }

    /// Search and scrape top results in one call.
    /// Returns search results with scraped markdown content.
    pub async fn search_and_scrape(
        &self,
        query: &str,
        num_results: usize,
        scraper: &crate::scrape::Markify,
    ) -> anyhow::Result<Vec<SerperScrapeResult>> {
        let search_results = self.search(query, num_results).await?;

        let mut scraped = Vec::new();

        for result in &search_results.results {
            match scraper
                .scrape(crate::scrape::ScrapeRequest {
                    url: result.link.clone(),
                    formats: vec![crate::transform::OutputFormat::Markdown],
                    mode: crate::extract::ExtractionMode::Article,
                    ..Default::default()
                })
                .await
            {
                Ok((scrape_result, meta)) => {
                    scraped.push(SerperScrapeResult {
                        title: result.title.clone(),
                        url: result.link.clone(),
                        snippet: result.snippet.clone(),
                        markdown: scrape_result.markdown,
                        fetch_ms: meta.fetch_ms,
                        engine: meta.engine,
                    });
                }
                Err(e) => {
                    warn!(url = %result.link, error = %e, "Failed to scrape search result");
                    scraped.push(SerperScrapeResult {
                        title: result.title.clone(),
                        url: result.link.clone(),
                        snippet: result.snippet.clone(),
                        markdown: None,
                        fetch_ms: 0,
                        engine: "error".to_string(),
                    });
                }
            }
        }

        Ok(scraped)
    }
}

/// Parse DuckDuckGo's HTML results page into organic results.
fn parse_ddg_results(html: &str, num_results: usize) -> Vec<SerperOrganicResult> {
    use scraper::{Html, Selector};

    let doc = Html::parse_document(html);
    let (Ok(result_sel), Ok(title_sel), Ok(snippet_sel)) = (
        Selector::parse(".result"),
        Selector::parse(".result__a"),
        Selector::parse(".result__snippet"),
    ) else {
        return Vec::new();
    };

    let mut out = Vec::new();
    for el in doc.select(&result_sel) {
        if out.len() >= num_results {
            break;
        }
        let Some(a) = el.select(&title_sel).next() else {
            continue;
        };
        let title = a.text().collect::<String>().trim().to_string();
        let link = a
            .value()
            .attr("href")
            .map(decode_ddg_link)
            .unwrap_or_default();
        if title.is_empty() || link.is_empty() {
            continue;
        }
        let snippet = el
            .select(&snippet_sel)
            .next()
            .map(|s| s.text().collect::<String>().trim().to_string())
            .filter(|s| !s.is_empty());
        out.push(SerperOrganicResult {
            position: Some(out.len() + 1),
            title,
            link,
            snippet,
        });
    }
    out
}

/// DuckDuckGo wraps result links as `//duckduckgo.com/l/?uddg=<encoded-url>`.
/// Extract and percent-decode the real destination URL.
fn decode_ddg_link(href: &str) -> String {
    let full = if let Some(rest) = href.strip_prefix("//") {
        format!("https://{rest}")
    } else {
        href.to_string()
    };
    if let Ok(parsed) = url::Url::parse(&full) {
        if let Some((_, value)) = parsed.query_pairs().find(|(k, _)| k == "uddg") {
            return value.into_owned();
        }
    }
    if full.starts_with("http") {
        full
    } else {
        href.to_string()
    }
}

/// Search result with scraped content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerperScrapeResult {
    pub title: String,
    pub url: String,
    pub snippet: Option<String>,
    pub markdown: Option<String>,
    pub fetch_ms: u64,
    pub engine: String,
}

/// Search configuration
#[derive(Debug, Clone)]
pub struct SearchConfig {
    pub api_key: String,
    pub max_results: usize,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            api_key: std::env::var("SERPER_API_KEY").unwrap_or_default(),
            max_results: 5,
        }
    }
}
