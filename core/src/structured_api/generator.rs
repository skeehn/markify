//! API spec generator — analyzes HTML and generates structured extraction rules.
//!
//! Uses heuristic page structure analysis to identify:
//! - Repeating elements (product lists, article feeds, card grids)
//! - Content blocks (article body, product details)
//! - Tables and structured data
//!
//! For request-specific extraction it calls an LLM (see `llm.rs`) to produce the
//! exact CSS selectors for the user's description, validated against the live
//! page. The heuristic strategies below are the no-key fallback.

use std::time::Instant;

use scraper::{Html, Selector};
use uuid::Uuid;

use crate::scrape::Markify;
use crate::structured_api::spec::*;
use crate::{ExtractionMode, OutputFormat, ScrapeRequest};

/// Generate an API spec from a URL + optional description.
pub async fn generate_api_spec(
    url: &str,
    description: Option<&str>,
    markify: &Markify,
) -> anyhow::Result<ApiSpec> {
    let start = Instant::now();

    // Scrape the page to get HTML
    let (result, _) = markify
        .scrape(ScrapeRequest {
            url: url.to_string(),
            formats: vec![OutputFormat::Both],
            mode: ExtractionMode::Full,
            include_links: true,
            include_raw_html: true,
            ..Default::default()
        })
        .await?;

    let html = result
        .raw_html
        .ok_or_else(|| anyhow::anyhow!("No raw HTML available"))?;
    // The parse.bot magic: ask an LLM for request-specific extraction rules.
    // Do this BEFORE building the (non-Send) HTML document so the async future
    // stays Send across the await. Falls back to heuristics on no-key/failure.
    let llm_endpoint = match (
        description,
        crate::structured_api::llm::LlmConfig::from_env(),
    ) {
        (Some(desc), Some(config)) => {
            match crate::structured_api::llm::generate_endpoint_via_llm(&html, desc, &config).await
            {
                Ok(endpoint) => Some(endpoint),
                Err(e) => {
                    tracing::warn!(error = %e, "LLM rule generation failed; using heuristics");
                    None
                }
            }
        }
        _ => None,
    };

    // Everything below is synchronous (no awaits): parse the document, validate
    // the LLM's selectors against it, and fall back to heuristics if they miss.
    let document = Html::parse_document(&html);
    let endpoints = match llm_endpoint {
        Some(endpoint) if count_container_matches(&document, &endpoint) > 0 => {
            tracing::debug!("using LLM-generated extraction rules");
            vec![endpoint]
        }
        _ => analyze_page_structure(&document, description),
    };

    // Generate response schema from endpoints
    let response_schema = generate_response_schema(&endpoints);

    // Generate a name for this API
    let name = description
        .map(|d| d.chars().take(50).collect::<String>())
        .unwrap_or_else(|| format!("API for {}", extract_domain(url)));

    let _execution_ms = start.elapsed().as_millis() as u64;

    Ok(ApiSpec {
        id: Uuid::new_v4().to_string(),
        url: url.to_string(),
        description: description.map(String::from),
        name,
        endpoints,
        response_schema,
        openapi_spec: None, // Generated on demand
        mcp_tool: None,     // Generated on demand
        created_at: chrono::Utc::now(),
        status: ApiStatus::Completed,
    })
}

/// How many elements the endpoint's container (or first field) selector matches.
fn count_container_matches(document: &Html, endpoint: &Endpoint) -> usize {
    let sel_str = endpoint.container_selector.clone().or_else(|| {
        endpoint
            .extraction_rules
            .first()
            .map(|r| r.selector.clone())
    });
    let Some(sel_str) = sel_str else {
        return 0;
    };
    match scraper::Selector::parse(&sel_str).ok() {
        Some(sel) => document.select(&sel).count(),
        None => 0,
    }
}

/// Analyze the page structure and generate extraction endpoints.
fn analyze_page_structure(document: &Html, _description: Option<&str>) -> Vec<Endpoint> {
    let mut endpoints = Vec::new();

    // Strategy 1: Find repeating list elements
    if let Some(list_endpoint) = find_repeating_lists(document) {
        endpoints.push(list_endpoint);
    }

    // Strategy 2: Find article/content blocks
    if let Some(article_endpoint) = find_article_content(document) {
        endpoints.push(article_endpoint);
    }

    // Strategy 3: Find tables
    if let Some(table_endpoint) = find_tables(document) {
        endpoints.push(table_endpoint);
    }

    // Strategy 4: Find card grids
    if let Some(card_endpoint) = find_card_grids(document) {
        endpoints.push(card_endpoint);
    }

    // Fallback: generic page extraction
    if endpoints.is_empty() {
        endpoints.push(generic_page_extraction(document));
    }

    endpoints
}

/// Find repeating list elements (e.g., product lists, article feeds).
fn find_repeating_lists(document: &Html) -> Option<Endpoint> {
    // Look for common list container patterns
    let list_patterns = [
        "ul > li",
        "ol > li",
        ".list > *",
        ".items > *",
        "[role='list'] > [role='listitem']",
    ];

    for pattern in &list_patterns {
        if let Ok(selector) = Selector::parse(pattern) {
            let items: Vec<_> = document.select(&selector).collect();
            if items.len() >= 3 {
                // Found a repeating list — analyze item structure
                let rules = extract_item_structure(&items);
                if !rules.is_empty() {
                    return Some(Endpoint {
                        container_selector: None,
                        name: "list_items".to_string(),
                        description: format!("Extract {} repeating items", items.len()),
                        extraction_rules: rules,
                        output_type: OutputType::List,
                        returns_list: true,
                    });
                }
            }
        }
    }

    None
}

/// Find article/content blocks.
fn find_article_content(document: &Html) -> Option<Endpoint> {
    let article_patterns = [
        "article",
        "main article",
        ".post",
        ".article",
        ".content",
        "[role='article']",
    ];

    for pattern in &article_patterns {
        if let Ok(selector) = Selector::parse(pattern) {
            if document.select(&selector).next().is_some() {
                let rules = vec![
                    ExtractionRule {
                        field: "title".to_string(),
                        selector: "h1".to_string(),
                        extract: "text".to_string(),
                        required: false,
                        field_type: FieldType::String,
                    },
                    ExtractionRule {
                        field: "content".to_string(),
                        selector: "article, .post, .content, main".to_string(),
                        extract: "text".to_string(),
                        required: false,
                        field_type: FieldType::String,
                    },
                    ExtractionRule {
                        field: "author".to_string(),
                        selector: ".author, [rel='author'], .byline".to_string(),
                        extract: "text".to_string(),
                        required: false,
                        field_type: FieldType::String,
                    },
                    ExtractionRule {
                        field: "date".to_string(),
                        selector: "time, .date, .published".to_string(),
                        extract: "text".to_string(),
                        required: false,
                        field_type: FieldType::Date,
                    },
                ];

                return Some(Endpoint {
                    container_selector: None,
                    name: "get_article".to_string(),
                    description: "Extract article content".to_string(),
                    extraction_rules: rules,
                    output_type: OutputType::Object,
                    returns_list: false,
                });
            }
        }
    }

    None
}

/// Find tables.
fn find_tables(document: &Html) -> Option<Endpoint> {
    if let Ok(selector) = Selector::parse("table") {
        if document.select(&selector).next().is_some() {
            let rules = vec![ExtractionRule {
                field: "table_data".to_string(),
                selector: "table".to_string(),
                extract: "html".to_string(),
                required: true,
                field_type: FieldType::List,
            }];

            return Some(Endpoint {
                container_selector: None,
                name: "get_table".to_string(),
                description: "Extract table data".to_string(),
                extraction_rules: rules,
                output_type: OutputType::List,
                returns_list: true,
            });
        }
    }

    None
}

/// Find card grids.
fn find_card_grids(document: &Html) -> Option<Endpoint> {
    let card_patterns = [
        ".card",
        ".grid > *",
        ".cards > *",
        "[role='grid'] > [role='gridcell']",
    ];

    for pattern in &card_patterns {
        if let Ok(selector) = Selector::parse(pattern) {
            let items: Vec<_> = document.select(&selector).collect();
            if items.len() >= 3 {
                let rules = extract_item_structure(&items);
                if !rules.is_empty() {
                    return Some(Endpoint {
                        container_selector: None,
                        name: "list_cards".to_string(),
                        description: format!("Extract {} card items", items.len()),
                        extraction_rules: rules,
                        output_type: OutputType::List,
                        returns_list: true,
                    });
                }
            }
        }
    }

    None
}

/// Generic page extraction fallback.
fn generic_page_extraction(_document: &Html) -> Endpoint {
    Endpoint {
        container_selector: None,
        name: "get_page".to_string(),
        description: "Extract page content".to_string(),
        extraction_rules: vec![
            ExtractionRule {
                field: "title".to_string(),
                selector: "title, h1".to_string(),
                extract: "text".to_string(),
                required: false,
                field_type: FieldType::String,
            },
            ExtractionRule {
                field: "description".to_string(),
                selector: "meta[name='description']".to_string(),
                extract: "attr:content".to_string(),
                required: false,
                field_type: FieldType::String,
            },
            ExtractionRule {
                field: "content".to_string(),
                selector: "main, body".to_string(),
                extract: "text".to_string(),
                required: false,
                field_type: FieldType::String,
            },
        ],
        output_type: OutputType::Object,
        returns_list: false,
    }
}

/// Extract common field structure from repeating items.
fn extract_item_structure(items: &[scraper::ElementRef]) -> Vec<ExtractionRule> {
    let mut rules = Vec::new();

    if items.is_empty() {
        return rules;
    }

    // Analyze first item to guess structure
    let first = &items[0];

    // Look for common patterns in the first item
    let patterns = [
        (
            "title",
            "h1, h2, h3, h4, .title, .name",
            "text",
            FieldType::String,
        ),
        ("link", "a", "href", FieldType::Url),
        ("image", "img", "src", FieldType::Url),
        (
            "description",
            "p, .desc, .summary",
            "text",
            FieldType::String,
        ),
        ("date", "time, .date", "text", FieldType::Date),
        ("price", ".price, .cost, .amount", "text", FieldType::Number),
    ];

    for (field, selector, extract, field_type) in &patterns {
        if let Ok(sel) = Selector::parse(selector) {
            if first.select(&sel).next().is_some() {
                rules.push(ExtractionRule {
                    field: field.to_string(),
                    selector: selector.to_string(),
                    extract: extract.to_string(),
                    required: false,
                    field_type: field_type.clone(),
                });
            }
        }
    }

    rules
}

/// Generate JSON Schema from endpoints.
fn generate_response_schema(endpoints: &[Endpoint]) -> serde_json::Value {
    let mut properties = serde_json::Map::new();

    for endpoint in endpoints {
        let mut endpoint_props = serde_json::Map::new();

        for rule in &endpoint.extraction_rules {
            let field_type = match rule.field_type {
                FieldType::String => "string",
                FieldType::Number => "number",
                FieldType::Boolean => "boolean",
                FieldType::Url => "string",
                FieldType::Date => "string",
                FieldType::List => "array",
            };

            endpoint_props.insert(
                rule.field.clone(),
                serde_json::json!({ "type": field_type }),
            );
        }

        if endpoint.returns_list {
            properties.insert(
                endpoint.name.clone(),
                serde_json::json!({
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": endpoint_props,
                    }
                }),
            );
        } else {
            properties.insert(
                endpoint.name.clone(),
                serde_json::json!({
                    "type": "object",
                    "properties": endpoint_props,
                }),
            );
        }
    }

    serde_json::json!({
        "type": "object",
        "properties": properties,
    })
}

/// Extract domain from URL.
fn extract_domain(url: &str) -> &str {
    url.trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_start_matches("www.")
        .split('/')
        .next()
        .unwrap_or(url)
}
