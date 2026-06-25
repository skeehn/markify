//! Fetch module: HTTP fetching with smart browser fallback.

pub mod browser;
pub mod http;
pub mod proxy;
pub mod router;

pub use http::HttpFetcher;
pub use proxy::{
    detect_bot_protection, detect_captcha, stealth_cdp_script, AntiBotConfig, BotProtectionType,
    BrowserFingerprint, CaptchaSolver, CaptchaType, ProviderConfig, ProviderType, ProxyEntry,
    ProxyPool, StealthConfig,
};
pub use router::FetchRouter;

use rand::Rng;
use reqwest::header::{HeaderMap, HeaderValue};

/// User-Agent pool for rotation
const USER_AGENTS: &[&str] = &[
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/134.0.0.0 Safari/537.36",
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/134.0.0.0 Safari/537.36",
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/134.0.0.0 Safari/537.36",
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.3 Safari/605.1.15",
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:136.0) Gecko/20100101 Firefox/136.0",
];

/// Get a random user-agent from the pool.
pub fn random_user_agent() -> &'static str {
    let mut rng = rand::thread_rng();
    USER_AGENTS[rng.gen_range(0..USER_AGENTS.len())]
}

/// Build default headers for HTTP requests.
pub fn default_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        reqwest::header::USER_AGENT,
        HeaderValue::from_static(USER_AGENTS[0]),
    );
    headers.insert(
        reqwest::header::ACCEPT,
        HeaderValue::from_static(
            "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,*/*;q=0.8",
        ),
    );
    headers.insert(
        reqwest::header::ACCEPT_LANGUAGE,
        HeaderValue::from_static("en-US,en;q=0.9"),
    );
    headers.insert(
        reqwest::header::ACCEPT_ENCODING,
        HeaderValue::from_static("gzip, deflate, br"),
    );
    headers.insert(
        reqwest::header::CACHE_CONTROL,
        HeaderValue::from_static("no-cache"),
    );
    headers
}

/// Fetch configuration
#[derive(Debug, Clone)]
pub struct FetchConfig {
    /// Request timeout in seconds
    pub timeout_secs: u64,
    /// Maximum redirect count
    pub max_redirects: usize,
    /// Whether to follow redirects
    pub follow_redirects: bool,
    /// Proxy URL (optional)
    pub proxy: Option<String>,
    /// Whether to verify SSL certificates
    pub danger_accept_invalid_certs: bool,
}

impl Default for FetchConfig {
    fn default() -> Self {
        Self {
            timeout_secs: 30,
            max_redirects: 10,
            follow_redirects: true,
            proxy: None,
            danger_accept_invalid_certs: false,
        }
    }
}

/// Result of a fetch operation
#[derive(Debug)]
pub struct FetchedPage {
    /// The HTML content
    pub html: String,
    /// HTTP status code
    pub status_code: u16,
    /// Final URL after redirects
    pub url: String,
    /// Content-Type header
    pub content_type: Option<String>,
    /// Encoding (from charset or Content-Type)
    pub encoding: String,
    /// Response headers
    pub headers: HeaderMap,
}
