//! API spec executor — runs extraction rules against HTML.

use std::time::Instant;

use scraper::{Html, Selector};

use crate::scrape::Markify;
use crate::structured_api::spec::*;
use crate::{ExtractionMode, OutputFormat, ScrapeRequest};

/// Execute an API spec against a URL.
///
/// When `max_pages > 1` and the endpoint has a `next_page_selector`, the executor
/// follows that link from page to page and aggregates rows across all pages
/// (capped at 20 pages). `max_pages = 1` (the default) extracts the single page.
pub async fn execute_api_spec(
    spec: &ApiSpec,
    endpoint_name: &str,
    markify: &Markify,
    override_url: Option<&str>,
    max_pages: usize,
) -> anyhow::Result<ExecutionResult> {
    let start = Instant::now();

    // Find the endpoint
    let endpoint = spec
        .endpoints
        .iter()
        .find(|e| e.name == endpoint_name)
        .ok_or_else(|| anyhow::anyhow!("Endpoint '{}' not found in API spec", endpoint_name))?;

    let max_pages = max_pages.clamp(1, 20);
    let first_url = override_url.unwrap_or(&spec.url).to_string();

    let mut current_url = first_url.clone();
    let mut data: Vec<serde_json::Value> = Vec::new();
    let mut pages_fetched = 0usize;

    loop {
        pages_fetched += 1;

        // Scrape the current page
        let (result, _) = markify
            .scrape(ScrapeRequest {
                url: current_url.clone(),
                formats: vec![OutputFormat::Both],
                mode: ExtractionMode::Full,
                include_raw_html: true,
                ..Default::default()
            })
            .await?;

        let html = result
            .raw_html
            .ok_or_else(|| anyhow::anyhow!("No raw HTML available"))?;

        // Synchronous block: the non-Send parsed document is created, used, and
        // dropped here so it never crosses the next `.await`.
        let next_url = {
            let document = Html::parse_document(&html);
            data.extend(execute_endpoint(&document, endpoint));
            endpoint
                .next_page_selector
                .as_deref()
                .and_then(|sel| resolve_next_url(&document, sel, &current_url))
        };

        if pages_fetched >= max_pages {
            break;
        }
        match next_url {
            Some(next) if next != current_url => current_url = next,
            _ => break,
        }
    }

    let execution_ms = start.elapsed().as_millis() as u64;

    Ok(ExecutionResult {
        api_id: spec.id.clone(),
        endpoint: endpoint_name.to_string(),
        data,
        execution_ms,
        source_url: first_url,
    })
}

/// Resolve the "next page" URL from `document` via `selector` (its `href`),
/// joined against `base_url` so relative links become absolute.
fn resolve_next_url(document: &Html, selector: &str, base_url: &str) -> Option<String> {
    let sel = Selector::parse(selector).ok()?;
    let href = document.select(&sel).next()?.value().attr("href")?;
    let base = url::Url::parse(base_url).ok()?;
    base.join(href).ok().map(|u| u.to_string())
}

/// Execute a single endpoint's extraction rules against a document.
fn execute_endpoint(document: &Html, endpoint: &Endpoint) -> Vec<serde_json::Value> {
    if endpoint.returns_list {
        // Find all matching elements and extract fields from each
        extract_list(document, endpoint)
    } else {
        // Extract fields from the whole document
        extract_object(document, endpoint)
    }
}

/// Parse `html` and run an endpoint's extraction rules against it. Synchronous
/// (the parsed document never crosses an await), so it is safe to call for
/// validation between LLM retries. Returns the extracted rows.
pub fn extract_from_html(html: &str, endpoint: &Endpoint) -> Vec<serde_json::Value> {
    let document = Html::parse_document(html);
    execute_endpoint(&document, endpoint)
}

/// Extract a list of objects from repeating container elements.
///
/// Uses `endpoint.container_selector` to find each repeating item, then applies
/// each field rule *relative to* that container. Falls back to the legacy
/// behaviour (the first rule's selector as the container) for older specs that
/// predate `container_selector`.
fn extract_list(document: &Html, endpoint: &Endpoint) -> Vec<serde_json::Value> {
    let mut results = Vec::new();

    let container_sel_str = endpoint.container_selector.clone().or_else(|| {
        endpoint
            .extraction_rules
            .first()
            .map(|r| r.selector.clone())
    });
    let Some(container_sel_str) = container_sel_str else {
        return results;
    };
    let Ok(container_sel) = Selector::parse(&container_sel_str) else {
        return results;
    };

    for container in document.select(&container_sel) {
        let mut obj = serde_json::Map::new();

        for rule in &endpoint.extraction_rules {
            let Ok(sel) = Selector::parse(&rule.selector) else {
                continue;
            };
            // Field selectors are relative to the container item; if a rule
            // targets the container element itself (e.g. the item *is* the
            // link), use the container.
            let element = container
                .select(&sel)
                .next()
                .or_else(|| match_self(container, &rule.selector, &container_sel_str));
            if let Some(element) = element {
                if let Some(v) = extract_from_element(&element, rule, &sel) {
                    obj.insert(rule.field.clone(), v);
                }
            }
        }

        if !obj.is_empty() {
            results.push(serde_json::Value::Object(obj));
        }
    }

    results
}

/// When a field rule targets the container element itself, return the container.
fn match_self<'a>(
    container: scraper::ElementRef<'a>,
    rule_selector: &str,
    container_selector: &str,
) -> Option<scraper::ElementRef<'a>> {
    if rule_selector == container_selector || rule_selector.trim() == ":scope" {
        Some(container)
    } else {
        None
    }
}

/// Extract a single object from the document.
fn extract_object(document: &Html, endpoint: &Endpoint) -> Vec<serde_json::Value> {
    let mut obj = serde_json::Map::new();

    for rule in &endpoint.extraction_rules {
        if let Ok(sel) = Selector::parse(&rule.selector) {
            if let Some(element) = document.select(&sel).next() {
                let value = extract_from_element(&element, rule, &sel);
                if let Some(v) = value {
                    obj.insert(rule.field.clone(), v);
                }
            }
        }
    }

    if !obj.is_empty() {
        vec![serde_json::Value::Object(obj)]
    } else {
        vec![]
    }
}

/// Extract a field value from an element based on the extraction rule.
fn extract_from_element(
    element: &scraper::ElementRef,
    rule: &ExtractionRule,
    _selector: &Selector,
) -> Option<serde_json::Value> {
    // Use the element that was already selected
    match rule.extract.as_str() {
        "text" => {
            let text = element.text().collect::<String>().trim().to_string();
            if text.is_empty() {
                None
            } else {
                Some(serde_json::Value::String(text))
            }
        }
        "href" => element
            .value()
            .attr("href")
            .map(|v| serde_json::Value::String(v.to_string())),
        "src" => element
            .value()
            .attr("src")
            .map(|v| serde_json::Value::String(v.to_string())),
        // `content` (e.g. <meta property="og:title" content="...">) is a very
        // common extraction target, so support it directly alongside attr:name.
        "content" => element
            .value()
            .attr("content")
            .map(|v| serde_json::Value::String(v.to_string())),
        "html" => {
            let html = element.html();
            if html.is_empty() {
                None
            } else {
                Some(serde_json::Value::String(html))
            }
        }
        s if s.starts_with("attr:") => {
            let attr_name = s.strip_prefix("attr:").unwrap_or("");
            element
                .value()
                .attr(attr_name)
                .map(|v| serde_json::Value::String(v.to_string()))
        }
        _ => None,
    }
}

#[cfg(test)]
mod pagination_tests {
    use super::*;

    #[test]
    fn resolves_relative_next_link_against_base() {
        let html = r#"<html><body><a class="morelink" href="news?p=2">More</a></body></html>"#;
        let doc = Html::parse_document(html);
        let next = resolve_next_url(&doc, ".morelink", "https://news.ycombinator.com/news");
        assert_eq!(
            next.as_deref(),
            Some("https://news.ycombinator.com/news?p=2")
        );
    }

    #[test]
    fn resolves_absolute_next_link() {
        let html = r#"<a rel="next" href="https://example.com/page/3">Next</a>"#;
        let doc = Html::parse_document(html);
        let next = resolve_next_url(&doc, "a[rel=next]", "https://example.com/page/2");
        assert_eq!(next.as_deref(), Some("https://example.com/page/3"));
    }

    #[test]
    fn missing_next_link_returns_none() {
        let html = r#"<html><body><p>last page</p></body></html>"#;
        let doc = Html::parse_document(html);
        assert!(resolve_next_url(&doc, ".morelink", "https://example.com/").is_none());
    }
}
