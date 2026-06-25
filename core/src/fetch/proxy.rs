//! Proxy Rotation & Anti-Bot Layer.
//!
//! Pluggable proxy providers with header rotation, stealth fingerprints,
//! and adaptive rate limiting for enterprise-grade web data extraction.
//!
//! Architecture:
//! 1. **ProxyProvider trait**: Interface for BrightData, Oxylabs, SmartProxy, etc.
//! 2. **ProxyPool**: Rotates proxies with health checking and failure recovery
//! 3. **HeaderRotator**: Rotates User-Agent, Accept, Sec-CH-UA, etc.
//! 4. **StealthConfig**: Browser stealth patches (remove webdriver flag, fake WebGL)
//! 5. **CaptchaSolver**: 2Captcha/AntiCaptcha integration

use std::sync::Mutex;
use std::time::{Duration, Instant};

use rand::seq::SliceRandom;
use tracing::{debug, warn};

/// Proxy provider type
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ProviderType {
    BrightData,
    Oxylabs,
    SmartProxy,
    IPRoyal,
    ProxyMesh,
    Custom(String),
}

/// Proxy entry in the pool
#[derive(Debug, Clone)]
pub struct ProxyEntry {
    pub provider: ProviderType,
    pub address: String,
    pub username: Option<String>,
    pub password: Option<String>,
    pub country: Option<String>,
    pub city: Option<String>,
    pub is_residential: bool,
    /// Health score 0.0-1.0
    pub health_score: f64,
    /// Number of successful requests
    pub success_count: u64,
    /// Number of failed requests
    pub failure_count: u64,
    /// Last used timestamp
    pub last_used: Option<Instant>,
    /// Cooldown until next use (for rate limiting)
    pub cooldown_until: Option<Instant>,
}

impl ProxyEntry {
    /// Get the proxy URL with auth embedded
    pub fn proxy_url(&self) -> String {
        if let (Some(user), Some(pass)) = (&self.username, &self.password) {
            format!("http://{}:{}@{}", user, pass, self.address)
        } else {
            format!("http://{}", self.address)
        }
    }
}

/// Proxy provider configuration
#[derive(Debug, Clone)]
pub struct ProviderConfig {
    pub provider_type: ProviderType,
    pub api_key: String,
    pub endpoint: String,
    pub is_residential: bool,
    /// Maximum concurrent sessions
    pub max_sessions: usize,
    /// Optional country targeting
    pub countries: Vec<String>,
}

/// Captcha solving provider
#[derive(Debug, Clone)]
pub enum CaptchaProvider {
    TwoCaptcha,
    AntiCaptcha,
    CapMonster,
}

/// Captcha solving configuration
#[derive(Debug, Clone)]
pub struct CaptchaConfig {
    pub provider: CaptchaProvider,
    pub api_key: String,
    /// Maximum wait time for solving (seconds)
    pub timeout_secs: u64,
    /// Maximum retry attempts
    pub max_retries: u32,
}

/// Stealth configuration for browser mode
#[derive(Debug, Clone)]
pub struct StealthConfig {
    /// Remove navigator.webdriver flag
    pub hide_webdriver: bool,
    /// Fake WebGL vendor/renderer
    pub fake_webgl: bool,
    /// Randomize canvas fingerprint
    pub noise_canvas: bool,
    /// Randomize audio fingerprint  
    pub noise_audio: bool,
    /// Spoof timezone
    pub spoof_timezone: bool,
    /// Spoof locale
    pub spoof_locale: bool,
    /// Enable Chrome DevTools anti-detection
    pub cdp_stealth: bool,
}

impl Default for StealthConfig {
    fn default() -> Self {
        Self {
            hide_webdriver: true,
            fake_webgl: true,
            noise_canvas: true,
            noise_audio: false,
            spoof_timezone: false,
            spoof_locale: false,
            cdp_stealth: true,
        }
    }
}

/// Fingerprint for header rotation
#[derive(Debug, Clone)]
pub struct BrowserFingerprint {
    pub user_agent: String,
    pub accept_language: String,
    pub accept_encoding: String,
    pub sec_ch_ua: String,
    pub sec_ch_ua_mobile: String,
    pub sec_ch_ua_platform: String,
    pub sec_fetch_dest: String,
    pub sec_fetch_mode: String,
    pub sec_fetch_site: String,
    pub dnt: Option<String>,
    pub upgrade_insecure_requests: String,
}

impl BrowserFingerprint {
    /// Generate a realistic Chrome fingerprint
    pub fn chrome() -> Self {
        let chrome_versions = ["123.0.0.0", "122.0.0.0", "121.0.0.0", "120.0.0.0"];
        let platform_versions = ["Windows 10", "Windows 11", "macOS 14.4.1", "macOS 14.3.0"];

        let chrome_ver = chrome_versions
            .choose(&mut rand::thread_rng())
            .unwrap_or(&chrome_versions[0]);
        let platform_ver = platform_versions
            .choose(&mut rand::thread_rng())
            .unwrap_or(&platform_versions[0]);
        let is_mac = platform_ver.contains("macOS");
        let mobile = if is_mac {
            "?0"
        } else {
            ["?0", "?1"]
                .choose(&mut rand::thread_rng())
                .unwrap_or(&"?0")
        };

        Self {
            user_agent: if is_mac {
                format!("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/{} Safari/537.36", chrome_ver)
            } else {
                format!("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/{} Safari/537.36", chrome_ver)
            },
            accept_language: [
                "en-US,en;q=0.9",
                "en-GB,en;q=0.9",
                "en-US,en;q=0.9,fr;q=0.8",
            ]
            .choose(&mut rand::thread_rng())
            .unwrap_or(&"en-US,en;q=0.9")
            .to_string(),
            accept_encoding: "gzip, deflate, br, zstd".to_string(),
            sec_ch_ua: format!(
                r#""Google Chrome";v="{}", "Chromium";v="{}", "Not/A)Brand";v="24""#,
                chrome_ver.split('.').next().unwrap_or("123"),
                chrome_ver.split('.').next().unwrap_or("123")
            ),
            sec_ch_ua_mobile: mobile.to_string(),
            sec_ch_ua_platform: format!(r#""{}""#, if is_mac { "macOS" } else { "Windows" }),
            sec_fetch_dest: "document".to_string(),
            sec_fetch_mode: "navigate".to_string(),
            sec_fetch_site: "none".to_string(),
            dnt: None,
            upgrade_insecure_requests: "1".to_string(),
        }
    }

    /// Generate a realistic Firefox fingerprint
    pub fn firefox() -> Self {
        let ff_versions = ["124.0", "123.0", "122.0"];
        let ff_ver = ff_versions
            .choose(&mut rand::thread_rng())
            .unwrap_or(&ff_versions[0]);

        Self {
            user_agent: format!(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:{}) Gecko/20100101 Firefox/{}",
                ff_ver, ff_ver
            ),
            accept_language: "en-US,en;q=0.5".to_string(),
            accept_encoding: "gzip, deflate, br".to_string(),
            sec_ch_ua: String::new(),
            sec_ch_ua_mobile: "?0".to_string(),
            sec_ch_ua_platform: String::new(),
            sec_fetch_dest: "document".to_string(),
            sec_fetch_mode: "navigate".to_string(),
            sec_fetch_site: "none".to_string(),
            dnt: Some("1".to_string()),
            upgrade_insecure_requests: "1".to_string(),
        }
    }

    /// Convert to reqwest header map entries
    pub fn to_headers(&self) -> Vec<(&'static str, String)> {
        let mut headers = vec![
            ("user-agent", self.user_agent.clone()),
            ("accept", "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,image/apng,*/*;q=0.8".to_string()),
            ("accept-language", self.accept_language.clone()),
            ("accept-encoding", self.accept_encoding.clone()),
            ("sec-fetch-dest", self.sec_fetch_dest.clone()),
            ("sec-fetch-mode", self.sec_fetch_mode.clone()),
            ("sec-fetch-site", self.sec_fetch_site.clone()),
            ("sec-fetch-user", "?1".to_string()),
            ("upgrade-insecure-requests", self.upgrade_insecure_requests.clone()),
        ];

        if !self.sec_ch_ua.is_empty() {
            headers.push(("sec-ch-ua", self.sec_ch_ua.clone()));
            headers.push(("sec-ch-ua-mobile", self.sec_ch_ua_mobile.clone()));
            headers.push(("sec-ch-ua-platform", self.sec_ch_ua_platform.clone()));
        }

        if let Some(dnt) = &self.dnt {
            headers.push(("dnt", dnt.clone()));
        }

        headers
    }
}

/// Proxy pool with rotation and health tracking
pub struct ProxyPool {
    /// All available proxies
    proxies: Vec<ProxyEntry>,
    /// Current index for round-robin
    current_index: usize,
    /// Lock for thread-safe access
    lock: Mutex<()>,
    /// Failed proxy cooldown duration
    cooldown_duration: Duration,
    /// Minimum health score to use a proxy
    min_health_score: f64,
}

impl ProxyPool {
    pub fn new(cooldown_duration: Duration, min_health_score: f64) -> Self {
        Self {
            proxies: Vec::new(),
            current_index: 0,
            lock: Mutex::new(()),
            cooldown_duration,
            min_health_score,
        }
    }

    /// Add a proxy to the pool
    pub fn add_proxy(&mut self, proxy: ProxyEntry) {
        self.proxies.push(proxy);
    }

    /// Add multiple proxies
    pub fn add_proxies(&mut self, proxies: Vec<ProxyEntry>) {
        self.proxies.extend(proxies);
    }

    /// Get the next healthy, non-cooldown proxy
    pub fn get_next(&mut self) -> Option<ProxyEntry> {
        let _lock = self.lock.lock().ok()?;

        if self.proxies.is_empty() {
            return None;
        }

        let now = Instant::now();
        let _start_idx = self.current_index;
        let len = self.proxies.len();

        for _ in 0..len {
            let idx = self.current_index % len;
            self.current_index += 1;

            let proxy = &self.proxies[idx];

            // Check health
            if proxy.health_score < self.min_health_score {
                continue;
            }

            // Check cooldown
            if let Some(cooldown) = proxy.cooldown_until {
                if now < cooldown {
                    continue;
                }
            }

            // Return a clone of the healthy proxy
            return Some(proxy.clone());
        }

        // All proxies unhealthy or on cooldown — reset all cooldowns and return best
        warn!("All proxies unhealthy, resetting cooldowns");
        for proxy in &mut self.proxies {
            proxy.cooldown_until = None;
        }

        self.proxies
            .iter()
            .max_by(|a, b| {
                a.health_score
                    .partial_cmp(&b.health_score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .cloned()
    }

    /// Report a successful request for a proxy
    pub fn report_success(&mut self, proxy_address: &str) {
        let _lock = self.lock.lock().ok();
        if let Some(proxy) = self.proxies.iter_mut().find(|p| p.address == proxy_address) {
            proxy.success_count += 1;
            proxy.health_score = calculate_health(proxy.success_count, proxy.failure_count);
            proxy.last_used = Some(Instant::now());
            proxy.cooldown_until = None;
            debug!(
                proxy = proxy_address,
                health = proxy.health_score,
                "Proxy success reported"
            );
        }
    }

    /// Report a failed request for a proxy
    pub fn report_failure(&mut self, proxy_address: &str, status_code: Option<u16>) {
        let _lock = self.lock.lock().ok();
        if let Some(proxy) = self.proxies.iter_mut().find(|p| p.address == proxy_address) {
            proxy.failure_count += 1;
            proxy.health_score = calculate_health(proxy.success_count, proxy.failure_count);
            proxy.last_used = Some(Instant::now());

            // Apply cooldown based on status code
            let cooldown = match status_code {
                Some(429) => Duration::from_secs(60),  // Rate limited — 1 min
                Some(403) => Duration::from_secs(300), // Forbidden — 5 min
                Some(503) => Duration::from_secs(30),  // Service unavailable — 30s
                _ => Duration::from_secs(10),          // Other — 10s
            };

            proxy.cooldown_until = Some(Instant::now() + cooldown);
            warn!(proxy = proxy_address, status = ?status_code, health = proxy.health_score, cooldown_secs = cooldown.as_secs(), "Proxy failure reported");
        }
    }

    /// Get pool stats
    pub fn stats(&self) -> ProxyPoolStats {
        let total = self.proxies.len();
        let healthy = self
            .proxies
            .iter()
            .filter(|p| p.health_score >= self.min_health_score)
            .count();
        let on_cooldown = self
            .proxies
            .iter()
            .filter(|p| p.cooldown_until.is_some_and(|c| Instant::now() < c))
            .count();
        let residential = self.proxies.iter().filter(|p| p.is_residential).count();

        ProxyPoolStats {
            total,
            healthy,
            on_cooldown,
            residential,
            datacenter: total - residential,
        }
    }
}

/// Calculate health score from success/failure counts
fn calculate_health(success: u64, failure: u64) -> f64 {
    let total = success + failure;
    if total == 0 {
        return 1.0; // Unknown — assume healthy
    }
    // Exponential decay: health drops quickly with failures
    let success_rate = success as f64 / total as f64;
    // Weight recent failures more heavily
    let decay_factor = if failure > 5 { 0.8 } else { 1.0 };
    success_rate * decay_factor
}

/// Proxy pool statistics
#[derive(Debug, Clone)]
pub struct ProxyPoolStats {
    pub total: usize,
    pub healthy: usize,
    pub on_cooldown: usize,
    pub residential: usize,
    pub datacenter: usize,
}

/// Captcha solver interface
pub trait CaptchaSolver: Send + Sync {
    /// Solve a captcha given the site key and page URL
    fn solve_recaptcha_v2(&self, site_key: &str, page_url: &str) -> anyhow::Result<String>;
    /// Solve hCaptcha
    fn solve_hcaptcha(&self, site_key: &str, page_url: &str) -> anyhow::Result<String>;
}

/// 2Captcha implementation (async via tokio)
pub struct TwoCaptchaSolver {
    pub api_key: String,
    pub timeout_secs: u64,
}

impl TwoCaptchaSolver {
    async fn submit_captcha(&self, params: &[(&str, &str)]) -> anyhow::Result<String> {
        let client = reqwest::Client::new();
        let mut query = vec![("key", self.api_key.as_str()), ("json", "1")];
        query.extend_from_slice(params);

        let response = client
            .get("http://2captcha.com/in.php")
            .query(&query)
            .send()
            .await?;

        let json: serde_json::Value = response.json().await?;
        if (json["status"].as_u64() != Some(1)) {
            anyhow::bail!("2Captcha submission failed: {}", json);
        }

        Ok(json["request"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("No captcha ID returned"))?
            .to_string())
    }

    async fn poll_captcha_result(&self, captcha_id: &str) -> anyhow::Result<String> {
        let client = reqwest::Client::new();
        let start = Instant::now();
        let timeout = Duration::from_secs(self.timeout_secs);

        loop {
            if start.elapsed() > timeout {
                anyhow::bail!("Captcha solving timed out after {}s", self.timeout_secs);
            }

            tokio::time::sleep(Duration::from_secs(5)).await;

            let response = client
                .get("http://2captcha.com/res.php")
                .query(&[
                    ("key", self.api_key.as_str()),
                    ("action", "get"),
                    ("id", captcha_id),
                    ("json", "1"),
                ])
                .send()
                .await?;

            let json: serde_json::Value = response.json().await?;
            if json["status"].as_u64() == Some(1) {
                return Ok(json["request"].as_str().unwrap_or("").to_string());
            }

            if json["request"].as_str() == Some("CAPCHA_NOT_READY") {
                continue;
            }

            anyhow::bail!("2Captcha error: {}", json["request"]);
        }
    }
}

impl CaptchaSolver for TwoCaptchaSolver {
    fn solve_recaptcha_v2(&self, site_key: &str, page_url: &str) -> anyhow::Result<String> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| anyhow::anyhow!("Failed to create tokio runtime: {}", e))?;

        rt.block_on(async {
            let captcha_id = self
                .submit_captcha(&[
                    ("method", "userrecaptcha"),
                    ("googlekey", site_key),
                    ("pageurl", page_url),
                ])
                .await?;
            self.poll_captcha_result(&captcha_id).await
        })
    }

    fn solve_hcaptcha(&self, site_key: &str, page_url: &str) -> anyhow::Result<String> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| anyhow::anyhow!("Failed to create tokio runtime: {}", e))?;

        rt.block_on(async {
            let captcha_id = self
                .submit_captcha(&[
                    ("method", "hcaptcha"),
                    ("sitekey", site_key),
                    ("pageurl", page_url),
                ])
                .await?;
            self.poll_captcha_result(&captcha_id).await
        })
    }
}

/// Anti-bot configuration
#[derive(Debug, Clone)]
pub struct AntiBotConfig {
    /// Maximum retries with different proxies
    pub max_proxy_retries: usize,
    /// Delay between retries (ms)
    pub retry_delay_ms: u64,
    /// Enable automatic proxy escalation (datacenter → residential)
    pub auto_escalate: bool,
    /// Detect CAPTCHA and trigger solving
    pub captcha_detection: bool,
    /// Detect bot protection (Cloudflare, Akamai, etc.)
    pub bot_protection_detection: bool,
}

impl Default for AntiBotConfig {
    fn default() -> Self {
        Self {
            max_proxy_retries: 3,
            retry_delay_ms: 1000,
            auto_escalate: true,
            captcha_detection: true,
            bot_protection_detection: true,
        }
    }
}

/// Detect if a response indicates bot protection
pub fn detect_bot_protection(html: &str, status_code: u16) -> Option<BotProtectionType> {
    let html_lower = html.to_lowercase();

    // Cloudflare
    if html_lower.contains("cloudflare")
        || html_lower.contains("__cfduid")
        || html_lower.contains("cf-browser-verification")
        || html_lower.contains("cf-chl-bypass")
        || status_code == 403 && html_lower.contains("checking your browser")
    {
        return Some(BotProtectionType::Cloudflare);
    }

    // Akamai
    if html_lower.contains("akamai") || html_lower.contains("_abck") || html_lower.contains("bm_sz")
    {
        return Some(BotProtectionType::Akamai);
    }

    // PerimeterX / HUMAN
    if html_lower.contains("perimeterx")
        || html_lower.contains("_px")
        || html_lower.contains("human") && html_lower.contains("challenge")
    {
        return Some(BotProtectionType::PerimeterX);
    }

    // Distil / Imperva
    if html_lower.contains("distil")
        || html_lower.contains("imperva")
        || html_lower.contains("incapsula")
    {
        return Some(BotProtectionType::Distil);
    }

    None
}

/// Types of bot protection
#[derive(Debug, Clone)]
pub enum BotProtectionType {
    Cloudflare,
    Akamai,
    PerimeterX,
    Distil,
}

impl std::fmt::Display for BotProtectionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BotProtectionType::Cloudflare => write!(f, "Cloudflare"),
            BotProtectionType::Akamai => write!(f, "Akamai"),
            BotProtectionType::PerimeterX => write!(f, "PerimeterX/HUMAN"),
            BotProtectionType::Distil => write!(f, "Distil/Imperva"),
        }
    }
}

/// Detect if a page has a CAPTCHA challenge
pub fn detect_captcha(html: &str) -> Option<CaptchaType> {
    let html_lower = html.to_lowercase();

    if html_lower.contains("g-recaptcha") || html_lower.contains("recaptcha") {
        return Some(CaptchaType::ReCaptchaV2);
    }

    if html_lower.contains("hcaptcha") || html_lower.contains("h-captcha") {
        return Some(CaptchaType::HCaptcha);
    }

    if html_lower.contains("turnstile") && html_lower.contains("cloudflare") {
        return Some(CaptchaType::CloudflareTurnstile);
    }

    if html_lower.contains("geetest") {
        return Some(CaptchaType::Geetest);
    }

    None
}

/// Types of CAPTCHA
#[derive(Debug, Clone)]
pub enum CaptchaType {
    ReCaptchaV2,
    HCaptcha,
    CloudflareTurnstile,
    Geetest,
}

/// Stealth script for CDP — injects into browser to hide automation signals
pub fn stealth_cdp_script() -> &'static str {
    r#"
    // Remove navigator.webdriver
    Object.defineProperty(navigator, 'webdriver', { get: () => undefined });
    
    // Fake plugins
    Object.defineProperty(navigator, 'plugins', {
        get: () => [1, 2, 3, 4, 5],
    });
    
    // Fake languages
    Object.defineProperty(navigator, 'languages', {
        get: () => ['en-US', 'en'],
    });
    
    // Fake permissions
    const originalQuery = window.navigator.permissions.query;
    window.navigator.permissions.query = (parameters) => (
        parameters.name === 'notifications' ?
            Promise.resolve({ state: Notification.permission }) :
            originalQuery(parameters)
    );
    
    // Override toString for functions that might be probed
    (() => {
        const element = document.createElement('div');
        const original = element.getAttribute;
        element.getAttribute = function(attr) {
            if (attr === 'webdriver') return null;
            return original.call(this, attr);
        };
    })();
    "#
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_browser_fingerprint_chrome() {
        let fp = BrowserFingerprint::chrome();
        assert!(fp.user_agent.contains("Chrome"));
        assert!(fp.user_agent.contains("Mozilla"));
        assert!(!fp.sec_ch_ua.is_empty());
    }

    #[test]
    fn test_browser_fingerprint_firefox() {
        let fp = BrowserFingerprint::firefox();
        assert!(fp.user_agent.contains("Firefox"));
        assert!(fp.user_agent.contains("Gecko"));
        assert!(fp.sec_ch_ua.is_empty()); // Firefox doesn't send sec-ch-ua
    }

    #[test]
    fn test_health_calculation() {
        assert_eq!(calculate_health(10, 0), 1.0);
        assert_eq!(calculate_health(9, 1), 0.9);
        assert_eq!(calculate_health(5, 5), 0.5);
        assert_eq!(calculate_health(0, 10), 0.0);
    }

    #[test]
    fn test_proxy_pool_rotation() {
        let mut pool = ProxyPool::new(Duration::from_secs(10), 0.5);

        pool.add_proxy(ProxyEntry {
            provider: ProviderType::BrightData,
            address: "proxy1.example.com:8080".to_string(),
            username: None,
            password: None,
            country: Some("US".to_string()),
            city: None,
            is_residential: false,
            health_score: 0.9,
            success_count: 100,
            failure_count: 5,
            last_used: None,
            cooldown_until: None,
        });

        pool.add_proxy(ProxyEntry {
            provider: ProviderType::Oxylabs,
            address: "proxy2.example.com:8080".to_string(),
            username: None,
            password: None,
            country: Some("UK".to_string()),
            city: None,
            is_residential: true,
            health_score: 0.95,
            success_count: 200,
            failure_count: 2,
            last_used: None,
            cooldown_until: None,
        });

        // Should get first proxy
        let proxy = pool.get_next().unwrap();
        assert_eq!(proxy.address, "proxy1.example.com:8080");

        // Should get second proxy next
        let proxy = pool.get_next().unwrap();
        assert_eq!(proxy.address, "proxy2.example.com:8080");
    }

    #[test]
    fn test_bot_protection_detection() {
        let cf_html = r#"<html><body><div id="cf-browser-verification"></div></body></html>"#;
        assert!(matches!(
            detect_bot_protection(cf_html, 403),
            Some(BotProtectionType::Cloudflare)
        ));

        let clean_html = r#"<html><body><h1>Hello World</h1></body></html>"#;
        assert!(detect_bot_protection(clean_html, 200).is_none());
    }
}
