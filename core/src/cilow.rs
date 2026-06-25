//! Cilow integration — export scraped data to Cilow's context engine.
//!
//! Cilow is a separate product with a custom Rust DB (graph + embedding + KV + time).
//! Markify exports clean web data that Cilow can index and use for context retrieval.

use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

use crate::scrape::{ScrapeMeta, ScrapeResult};

/// Cilow export client
pub struct CilowClient {
    client: Client,
    base_url: String,
    api_key: Option<String>,
}

impl CilowClient {
    pub fn new(base_url: String, api_key: Option<String>) -> Self {
        Self {
            client: Client::new(),
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key,
        }
    }

    /// Create from environment variables
    pub fn from_env() -> Option<Self> {
        let base_url = std::env::var("CILOW_API_URL").ok()?;
        let api_key = std::env::var("CILOW_API_KEY").ok();
        Some(Self::new(base_url, api_key))
    }

    /// Export scraped data to Cilow as a document.
    pub async fn export_document(
        &self,
        scrape_result: &ScrapeResult,
        scrape_meta: &ScrapeMeta,
        tags: Option<Vec<String>>,
    ) -> anyhow::Result<CilowExportResult> {
        let content = scrape_result.markdown.clone().unwrap_or_default();

        if content.is_empty() {
            return Err(anyhow::anyhow!("No content to export"));
        }

        let metadata = serde_json::json!({
            "source": scrape_result.url,
            "title": scrape_result.metadata.as_ref().and_then(|m| m.title.clone()),
            "description": scrape_result.metadata.as_ref().and_then(|m| m.description.clone()),
            "language": scrape_result.metadata.as_ref().and_then(|m| m.language.clone()),
            "scrape_engine": scrape_meta.engine,
            "scrape_ms": scrape_meta.fetch_ms,
            "exported_at": chrono::Utc::now().to_rfc3339(),
            "tags": tags.clone().unwrap_or_default(),
        });

        let payload = serde_json::json!({
            "content": content,
            "metadata": metadata,
            "source_url": scrape_result.url,
            "source_type": "web_scrape",
        });

        debug!(
            url = %scrape_result.url,
            content_len = content.len(),
            "Exporting to Cilow"
        );

        let mut request = self
            .client
            .post(format!("{}/api/documents", self.base_url))
            .header("Content-Type", "application/json");

        if let Some(key) = &self.api_key {
            request = request.header("Authorization", format!("Bearer {}", key));
        }

        let response = request.json(&payload).send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Cilow export failed: {} - {}", status, body);
        }

        let result: CilowExportResult = response.json().await?;

        info!(
            url = %scrape_result.url,
            document_id = result.document_id,
            "Exported to Cilow"
        );

        Ok(result)
    }

    /// Batch export multiple scrape results to Cilow.
    pub async fn export_batch(
        &self,
        results: &[(ScrapeResult, ScrapeMeta)],
        tags: Option<Vec<String>>,
    ) -> anyhow::Result<Vec<CilowExportResult>> {
        let mut exported = Vec::new();

        for (scrape_result, scrape_meta) in results {
            match self
                .export_document(scrape_result, scrape_meta, tags.clone())
                .await
            {
                Ok(result) => exported.push(result),
                Err(e) => {
                    debug!(error = %e, "Failed to export document to Cilow");
                }
            }
        }

        Ok(exported)
    }
}

/// Result of a Cilow export
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CilowExportResult {
    /// Document ID in Cilow
    pub document_id: String,
    /// Whether the export was successful
    pub success: bool,
    /// Number of tokens/indexed
    pub indexed_size: Option<usize>,
    /// Export timestamp
    pub exported_at: Option<String>,
}

/// Cilow export configuration
#[derive(Debug, Clone)]
pub struct CilowConfig {
    pub base_url: String,
    pub api_key: Option<String>,
}

impl Default for CilowConfig {
    fn default() -> Self {
        Self {
            base_url: std::env::var("CILOW_API_URL")
                .unwrap_or_else(|_| "http://localhost:8000".to_string()),
            api_key: std::env::var("CILOW_API_KEY").ok(),
        }
    }
}
