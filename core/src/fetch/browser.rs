//! Headless browser fetcher using chromiumoxide.
//!
//! Handles JS-rendered pages, SPAs, and pages requiring interaction.
//! This is the slow path but necessary for the 15-20% of pages
//! that render content client-side.

use crate::fetch::{FetchConfig, FetchedPage};

/// Browser-based page fetcher.
/// Wraps chromiumoxide for headless Chrome automation.
#[cfg(feature = "browser")]
pub struct BrowserFetcher {
    // chromiumoxide::Browser handle
    // Managed connection to Chrome DevTools Protocol
    chrome_path: Option<std::path::PathBuf>,
}

#[cfg(feature = "browser")]
impl BrowserFetcher {
    pub fn new(config: &FetchConfig) -> Self {
        Self {
            chrome_path: None, // Will use system Chrome/Chromium
        }
    }

    /// Fetch a URL using a headless browser.
    pub async fn fetch(
        &self,
        url: &str,
        wait_for_selector: Option<&str>,
        timeout_ms: u64,
    ) -> anyhow::Result<FetchedPage> {
        use chromiumoxide::{
            browser::{Browser, BrowserConfig},
            cdp::browser_protocol::page::ScreenshotParams,
            handler::viewport::Viewport,
        };

        debug!(url = %url, "Fetching page via headless browser");

        let (browser, mut handler) = Browser::launch(
            BrowserConfig::builder()
                .headless()
                .window_size(1920, 1080)
                .build()
                .unwrap(),
        )
        .await?;

        // Spawn handler
        let _handle = tokio::spawn(async move {
            while let Some(h) = handler.next().await {
                if h.is_err() {
                    break;
                }
            }
        });

        let page = browser.new_page(url).await?;

        // Wait for content
        if let Some(selector) = wait_for_selector {
            let _ = page.wait_for_selector(selector).await;
        } else {
            // Wait for network to be idle
            let _ = page.wait_for_navigation().await;
        }

        let html = page.get_content().await?;
        let status_code = 200; // chromiumoxide doesn't expose status easily

        debug!(
            url = %url,
            bytes = html.len(),
            "Browser fetch complete"
        );

        Ok(FetchedPage {
            html,
            status_code,
            url: url.to_string(),
            content_type: Some("text/html".to_string()),
            encoding: "utf-8".to_string(),
            headers: Default::default(),
        })
    }
}

#[cfg(not(feature = "browser"))]
pub struct BrowserFetcher;

#[cfg(not(feature = "browser"))]
impl BrowserFetcher {
    pub fn new(_config: &FetchConfig) -> Self {
        Self
    }

    pub async fn fetch(
        &self,
        _url: &str,
        _wait_for_selector: Option<&str>,
        _timeout_ms: u64,
    ) -> anyhow::Result<FetchedPage> {
        anyhow::bail!("Browser feature not enabled. Build with --features browser")
    }
}
