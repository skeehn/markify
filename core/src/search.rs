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

/// Serper search client
pub struct SearchClient {
    client: Client,
    api_key: String,
    base_url: String,
}

impl SearchClient {
    pub fn new(api_key: String) -> Self {
        Self {
            client: Client::new(),
            api_key,
            base_url: "https://google.serper.dev".to_string(),
        }
    }

    /// Search the web and return organic results.
    pub async fn search(&self, query: &str, num_results: usize) -> anyhow::Result<SearchResult> {
        debug!(query = %query, count = num_results, "Searching via Serper");

        let response = self
            .client
            .post(format!("{}/search", self.base_url))
            .header("X-API-KEY", &self.api_key)
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

        debug!(
            results = organic.len(),
            query = %search_query,
            "Search complete"
        );

        Ok(SearchResult {
            query: search_query,
            count: organic.len(),
            results: organic,
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
