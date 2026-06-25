//! API spec executor — runs extraction rules against HTML.

use std::time::Instant;

use scraper::{Html, Selector};

use crate::scrape::Markify;
use crate::structured_api::spec::*;
use crate::{ExtractionMode, OutputFormat, ScrapeRequest};

/// Execute an API spec against a URL.
pub async fn execute_api_spec(
    spec: &ApiSpec,
    endpoint_name: &str,
    markify: &Markify,
    override_url: Option<&str>,
) -> anyhow::Result<ExecutionResult> {
    let start = Instant::now();

    let url = override_url.unwrap_or(&spec.url);

    // Find the endpoint
    let endpoint = spec
        .endpoints
        .iter()
        .find(|e| e.name == endpoint_name)
        .ok_or_else(|| anyhow::anyhow!("Endpoint '{}' not found in API spec", endpoint_name))?;

    // Scrape the page
    let (result, _) = markify
        .scrape(ScrapeRequest {
            url: url.to_string(),
            formats: vec![OutputFormat::Both],
            mode: ExtractionMode::Full,
            include_raw_html: true,
            ..Default::default()
        })
        .await?;

    let html = result
        .raw_html
        .ok_or_else(|| anyhow::anyhow!("No raw HTML available"))?;
    let document = Html::parse_document(&html);

    // Execute extraction rules
    let data = execute_endpoint(&document, endpoint);

    let execution_ms = start.elapsed().as_millis() as u64;

    Ok(ExecutionResult {
        api_id: spec.id.clone(),
        endpoint: endpoint_name.to_string(),
        data,
        execution_ms,
        source_url: url.to_string(),
    })
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

/// Extract a list of objects from repeating elements.
fn extract_list(document: &Html, endpoint: &Endpoint) -> Vec<serde_json::Value> {
    let mut results = Vec::new();

    // Find all matching container elements
    let mut containers = Vec::new();
    for rule in &endpoint.extraction_rules {
        if let Ok(sel) = Selector::parse(&rule.selector) {
            for elem in document.select(&sel) {
                containers.push(elem);
            }
        }
    }

    if containers.is_empty() {
        return results;
    }

    // For each container, extract fields
    for container in containers {
        let mut obj = serde_json::Map::new();

        for rule in &endpoint.extraction_rules {
            if let Ok(sel) = Selector::parse(&rule.selector) {
                // Try to find matching element within container
                if let Some(element) = container.select(&sel).next() {
                    let value = extract_from_element(&element, rule, &sel);
                    if let Some(v) = value {
                        obj.insert(rule.field.clone(), v);
                    }
                }
            }
        }

        if !obj.is_empty() {
            results.push(serde_json::Value::Object(obj));
        }
    }

    results
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
