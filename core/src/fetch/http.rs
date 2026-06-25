//! HTTP-only fetcher using reqwest.
//!
//! This is the fast path — handles 80% of static pages.

use reqwest::{header::HeaderMap, Client};
use tracing::debug;

use crate::fetch::{default_headers, random_user_agent, FetchConfig, FetchedPage};
use std::time::Duration;

/// HTTP-only page fetcher
pub struct HttpFetcher {
    client: Client,
}

impl HttpFetcher {
    pub fn new(config: &FetchConfig) -> Self {
        let mut builder = Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .default_headers(default_headers())
            .gzip(true)
            .brotli(true);

        if !config.follow_redirects {
            builder = builder.redirect(reqwest::redirect::Policy::none());
        } else if config.max_redirects > 0 {
            builder = builder.redirect(reqwest::redirect::Policy::limited(config.max_redirects));
        }

        if config.danger_accept_invalid_certs {
            builder = builder.danger_accept_invalid_certs(true);
        }

        if let Some(proxy_url) = &config.proxy {
            if let Ok(proxy) = reqwest::Proxy::all(proxy_url) {
                builder = builder.proxy(proxy);
            }
        }

        let client = builder.build().expect("Failed to build HTTP client");

        Self { client }
    }

    /// Fetch a URL and return the HTML content.
    pub async fn fetch(
        &self,
        url: &str,
        headers: Option<HeaderMap>,
    ) -> anyhow::Result<FetchedPage> {
        let mut request = self.client.get(url);

        if let Some(extra_headers) = headers {
            request = request.headers(extra_headers);
        }

        // Rotate user-agent
        request = request.header(reqwest::header::USER_AGENT, random_user_agent());

        debug!(url = %url, "Fetching page via HTTP");

        let response = request.send().await?;
        let status_code = response.status().as_u16();
        let final_url = response.url().to_string();
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());
        let headers = response.headers().clone();

        // Detect encoding
        let encoding = detect_encoding(&content_type, &headers);

        let html = response.text().await?;

        debug!(
            url = %url,
            status = status_code,
            bytes = html.len(),
            encoding = %encoding,
            "HTTP fetch complete"
        );

        Ok(FetchedPage {
            html,
            status_code,
            url: final_url,
            content_type,
            encoding,
            headers,
        })
    }

    /// Check if a page likely requires JavaScript rendering.
    /// Heuristics: very small HTML with no content, SPA shell indicators.
    pub fn likely_needs_browser(&self, html: &str) -> bool {
        // If HTML is tiny (<500 bytes), it's probably not a full page
        // Don't mistake small but complete pages (like example.com) for SPAs
        if html.len() < 500 {
            return false;
        }

        // If HTML is very small (<2000) and has no meaningful content
        if html.len() < 2000 {
            // Check if it's just a shell with scripts
            let script_count = html.matches("<script").count();
            let has_content = html.contains("<article")
                || html.contains("<main")
                || html.contains("<p>")
                || html.contains("class=\"content\"")
                || html.contains("class=\"post\"")
                || html.contains("<h1")
                || html.contains("<h2");
            if script_count > 3 && !has_content {
                return true;
            }
            return false;
        }

        // Count script tags vs content
        let script_count = html.matches("<script").count();
        let noscript_count = html.matches("<noscript").count();

        // If there are many scripts but little noscript content, it's likely JS-rendered
        if script_count > 5 && noscript_count == 0 {
            // But check if there's actual HTML content
            let has_content = html.contains("<article")
                || html.contains("<main")
                || html.contains("<p>")
                || html.contains("<h1")
                || html.contains("<h2")
                || html.contains("class=\"entry\"")
                || html.contains("class=\"content\"");
            if has_content {
                return false;
            }
            return true;
        }

        // Check for common SPA indicators
        let has_root_div = html.contains("id=\"root\"") || html.contains("id=\"app\"");
        let has_no_content = !html.contains("<article")
            && !html.contains("<main")
            && !html.contains("class=\"content\"")
            && !html.contains("class=\"post\"")
            && !html.contains("<h1")
            && html.len() < 3000;

        has_root_div && has_no_content
    }
}

/// Detect character encoding from Content-Type header or meta tags.
fn detect_encoding(content_type: &Option<String>, headers: &HeaderMap) -> String {
    // Check Content-Type charset
    if let Some(ct) = content_type {
        if let Some(start) = ct.find("charset=") {
            let charset = ct[start + 8..].trim().split(';').next().unwrap_or("");
            if !charset.is_empty() {
                return charset.to_string();
            }
        }
    }

    // Check Content-Type header directly
    if let Some(ct) = headers.get(reqwest::header::CONTENT_TYPE) {
        if let Ok(ct_str) = ct.to_str() {
            if let Some(start) = ct_str.find("charset=") {
                let charset = ct_str[start + 8..].trim().split(';').next().unwrap_or("");
                if !charset.is_empty() {
                    return charset.to_string();
                }
            }
        }
    }

    "utf-8".to_string()
}
