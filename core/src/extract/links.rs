//! Link extraction with relevance scoring.

use scraper::{ElementRef, Html, Selector};
use tracing::debug;
use url::Url;

use crate::extract::LinkInfo;

/// Extract all links from an HTML document with relevance scores.
pub fn extract_links(html: &str, base_url: Option<&str>) -> Vec<LinkInfo> {
    let document = Html::parse_document(html);
    let selector = Selector::parse("a[href]").unwrap();

    let base_domain = base_url.and_then(|u| {
        Url::parse(u)
            .ok()
            .and_then(|u| u.host_str().map(|h| h.to_string()))
    });

    let links: Vec<LinkInfo> = document
        .select(&selector)
        .filter_map(|element: ElementRef| {
            let href = element.value().attr("href")?;
            let text = element.text().collect::<String>().trim().to_string();

            // Skip javascript: and anchor-only links
            if href.starts_with("javascript:") || (href.starts_with('#') && text.is_empty()) {
                return None;
            }

            let resolved_url = resolve_url(href, base_url);
            let is_internal = is_internal_link(&resolved_url, &base_domain);
            let score: f64 = calculate_link_score(&element, &text, &resolved_url);

            Some(LinkInfo {
                text,
                url: resolved_url,
                score,
                is_internal,
            })
        })
        .collect();

    debug!(link_count = links.len(), "Links extracted");
    links
}

/// Resolve a relative URL against a base.
fn resolve_url(url: &str, base: Option<&str>) -> String {
    if url.starts_with("http://") || url.starts_with("https://") || url.starts_with("//") {
        return url.to_string();
    }

    if let Some(base_url) = base {
        if let Ok(base) = Url::parse(base_url) {
            if let Ok(resolved) = base.join(url) {
                return resolved.to_string();
            }
        }
    }

    url.to_string()
}

/// Check if a link is internal (same domain).
fn is_internal_link(url: &str, base_domain: &Option<String>) -> bool {
    if let Some(domain) = base_domain {
        if let Ok(parsed) = Url::parse(url) {
            return parsed.host_str() == Some(domain.as_str());
        }
    }
    url.starts_with('/') || !url.contains("://")
}

/// Calculate relevance score for a link (0.0 - 1.0).
fn calculate_link_score(element: &ElementRef, text: &str, _url: &str) -> f64 {
    let mut score: f64 = 0.3; // Base score

    // Text length bonus
    let text_len = text.len();
    if text_len > 3 {
        score += 0.2;
    }
    if text_len > 10 {
        score += 0.1;
    }

    // Context bonus: check parent element tag name
    if let Some(parent_handle) = element.parent() {
        if let scraper::Node::Element(el) = parent_handle.value() {
            match el.name() {
                "article" | "main" | "section" => score += 0.2,
                "nav" | "footer" | "header" => score -= 0.15,
                "aside" => score -= 0.1,
                _ => {}
            }
        }
    }

    score.clamp(0.0, 1.0)
}
