//! Smart fetch router: tries HTTP first, falls back to browser for JS-rendered pages.

use tracing::{debug, info};

use crate::fetch::{browser::BrowserFetcher, FetchConfig, FetchedPage, HttpFetcher};

/// Which engine handled the request
#[derive(Debug, Clone)]
pub enum FetchEngine {
    Http,
    Browser,
}

/// Smart router that picks the best fetch strategy.
pub struct FetchRouter {
    http: HttpFetcher,
    browser: BrowserFetcher,
}

impl FetchRouter {
    pub fn new(config: &FetchConfig) -> Self {
        Self {
            http: HttpFetcher::new(config),
            browser: BrowserFetcher::new(config),
        }
    }

    /// Fetch a URL, automatically picking the best strategy.
    /// Tries HTTP first. If the page looks JS-rendered, retries with browser.
    pub async fn fetch(
        &self,
        url: &str,
        wait_for_selector: Option<&str>,
        timeout_ms: u64,
        force_browser: bool,
    ) -> anyhow::Result<(FetchedPage, FetchEngine)> {
        // If browser is forced, skip HTTP
        if force_browser {
            info!(url = %url, "Forcing browser fetch");
            match self.browser.fetch(url, wait_for_selector, timeout_ms).await {
                Ok(page) => return Ok((page, FetchEngine::Browser)),
                Err(e) => {
                    // Graceful fallback: if browser not available, return error with hint
                    if e.to_string().contains("Browser feature not enabled") {
                        anyhow::bail!("Browser rendering not available. Build with --features browser, or use a static page.");
                    }
                    anyhow::bail!("Browser fetch failed: {}", e);
                }
            }
        }

        // Try HTTP first
        debug!(url = %url, "Trying HTTP fetch");
        let result = self.http.fetch(url, None).await;

        match result {
            Ok(page) => {
                // Check if this looks like a JS-rendered shell
                if self.http.likely_needs_browser(&page.html) {
                    info!(url = %url, "Page appears JS-rendered, falling back to browser");
                    match self.browser.fetch(url, wait_for_selector, timeout_ms).await {
                        Ok(browser_page) => Ok((browser_page, FetchEngine::Browser)),
                        Err(e) => {
                            // Graceful fallback: return HTTP result with warning
                            debug!(error = %e, "Browser fallback failed, returning HTTP result");
                            Ok((page, FetchEngine::Http))
                        }
                    }
                } else {
                    Ok((page, FetchEngine::Http))
                }
            }
            Err(e) => {
                debug!(url = %url, error = %e, "HTTP fetch failed, trying browser");
                match self.browser.fetch(url, wait_for_selector, timeout_ms).await {
                    Ok(browser_page) => Ok((browser_page, FetchEngine::Browser)),
                    Err(_) => {
                        // Both failed, return the original HTTP error
                        Err(e)
                    }
                }
            }
        }
    }
}
