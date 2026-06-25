//! Metadata extraction: Open Graph, Twitter Cards, JSON-LD, canonical URL, language.

use scraper::{Html, Selector};
use tracing::debug;

use crate::extract::Metadata;

/// Extract all metadata from an HTML document.
pub fn extract_metadata(html: &str, base_url: Option<&str>) -> Metadata {
    let document = Html::parse_document(html);

    let mut metadata = Metadata::default();

    // Extract <title>
    if let Ok(title_sel) = Selector::parse("title") {
        if let Some(title) = document
            .select(&title_sel)
            .next()
            .map(|e| e.text().collect::<String>())
        {
            metadata.title = Some(title.trim().to_string());
        }
    }

    // Extract meta tags
    let meta_selector = Selector::parse("meta").unwrap();

    for element in document.select(&meta_selector) {
        let name = element
            .value()
            .attr("name")
            .or_else(|| element.value().attr("property"))
            .or_else(|| element.value().attr("itemprop"))
            .unwrap_or("");

        let content = element.value().attr("content").unwrap_or("");

        if content.is_empty() {
            continue;
        }

        match name {
            // Open Graph
            "og:title" => metadata.title = Some(content.to_string()),
            "og:description" => metadata.description = Some(content.to_string()),
            "og:image" => metadata.image = Some(resolve_url(content, base_url)),
            "og:url" => metadata.url = Some(content.to_string()),
            "og:site_name" => metadata.site_name = Some(content.to_string()),
            "og:type" => metadata.og_type = Some(content.to_string()),

            // Twitter Cards
            "twitter:title" if metadata.title.is_none() => {
                metadata.title = Some(content.to_string());
            }
            "twitter:description" if metadata.description.is_none() => {
                metadata.description = Some(content.to_string());
            }
            "twitter:image" if metadata.image.is_none() => {
                metadata.image = Some(resolve_url(content, base_url));
            }
            "twitter:card" => metadata.twitter_card = Some(content.to_string()),

            // Standard meta
            "description" if metadata.description.is_none() => {
                metadata.description = Some(content.to_string());
            }
            "author" => metadata.author = Some(content.to_string()),

            _ => {}
        }
    }

    // Extract canonical URL
    if let Ok(sel) = Selector::parse("link[rel=\"canonical\"]") {
        for element in document.select(&sel) {
            if let Some(href) = element.value().attr("href") {
                metadata.canonical_url = Some(href.to_string());
                break;
            }
        }
    }

    // Extract language
    if let Ok(html_sel) = Selector::parse("html") {
        if let Some(html_elem) = document.select(&html_sel).next() {
            if let Some(lang) = html_elem.value().attr("lang") {
                metadata.language = Some(lang.to_string());
            }
        }
    }

    // Extract favicon
    if let Ok(link_sel) = Selector::parse("link") {
        for element in document.select(&link_sel) {
            let rel = element.value().attr("rel").unwrap_or("");
            if rel.contains("icon") {
                if let Some(href) = element.value().attr("href") {
                    metadata.favicon = Some(resolve_url(href, base_url));
                    break;
                }
            }
        }
    }

    // Extract JSON-LD structured data
    if let Some(schema) = extract_jsonld(&document) {
        metadata.schema_org = Some(schema);
    }

    debug!(
        title = ?metadata.title,
        description_len = metadata.description.as_ref().map(|s| s.len()),
        "Metadata extracted"
    );

    metadata
}

/// Extract JSON-LD structured data.
fn extract_jsonld(document: &Html) -> Option<serde_json::Value> {
    let selector = Selector::parse("script[type=\"application/ld+json\"]").ok()?;

    let mut schemas: Vec<serde_json::Value> = Vec::new();

    for element in document.select(&selector) {
        let text = element.text().collect::<String>();
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) {
            schemas.push(value);
        }
    }

    if schemas.len() == 1 {
        Some(schemas.pop().unwrap())
    } else if !schemas.is_empty() {
        Some(serde_json::Value::Array(schemas))
    } else {
        None
    }
}

/// Resolve a relative URL against a base URL.
fn resolve_url(url: &str, base: Option<&str>) -> String {
    if url.starts_with("http://") || url.starts_with("https://") || url.starts_with("//") {
        return url.to_string();
    }

    if let Some(base_url) = base {
        if let Ok(base) = url::Url::parse(base_url) {
            if let Ok(resolved) = base.join(url) {
                return resolved.to_string();
            }
        }
    }

    url.to_string()
}
