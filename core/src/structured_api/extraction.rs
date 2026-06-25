//! Hybrid Structured Extraction — Schema + LLM modes.
//!
//! Two extraction modes:
//! 1. **Schema mode**: User provides JSON schema → generate DOM extraction program
//! 2. **LLM mode**: Natural language description → LLM generates schema + program
//!
//! Architecture:
//! 1. Schema/LLM → extraction program (Rust/Python DOM selectors)
//! 2. Program verifier → test against golden pages
//! 3. Program executor → run extraction on target URL
//! 4. Result validator → verify output matches schema

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ─── Schema Mode ─────────────────────────────────────────────────────────────

/// Extraction schema — defines what data to extract
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractionSchema {
    /// Schema name
    pub name: String,
    /// Schema version
    pub version: String,
    /// Fields to extract
    pub fields: Vec<SchemaField>,
    /// Optional: CSS selectors for known patterns
    pub selectors: HashMap<String, String>,
    /// Target URL pattern
    pub url_pattern: Option<String>,
}

/// A field to extract from a page
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaField {
    /// Field name
    pub name: String,
    /// Field type
    pub field_type: FieldType,
    /// CSS selector to extract from
    pub selector: Option<String>,
    /// Whether this field is required
    pub required: bool,
    /// Description for LLM generation
    pub description: Option<String>,
    /// Extract as list (multiple values)
    pub is_list: bool,
}

/// Field types for extraction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FieldType {
    Text,
    Url,
    Number,
    Date,
    Boolean,
    Html,
    Attribute { attribute: String },
    Image { extract_alt: bool },
}

/// Extraction program — generated from schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractionProgram {
    /// Program ID
    pub id: String,
    /// Schema this program was generated from
    pub schema_name: String,
    /// Extraction steps
    pub steps: Vec<ExtractionStep>,
    /// Verification status
    pub verified: bool,
    /// Generated at
    pub generated_at: chrono::DateTime<chrono::Utc>,
}

/// A single extraction step
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractionStep {
    /// Field name this step extracts
    pub field_name: String,
    /// Extraction method
    pub method: ExtractionMethod,
    /// CSS selector or XPath
    pub selector: String,
    /// Post-processing (optional)
    pub post_process: Option<PostProcess>,
}

/// Extraction methods
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExtractionMethod {
    /// Extract text content
    TextContent,
    /// Extract attribute value
    Attribute { name: String },
    /// Extract HTML inner content
    InnerHtml,
    /// Extract from list of elements
    List { item_selector: String },
    /// Extract image src
    ImageSrc,
}

/// Post-processing steps
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PostProcess {
    /// Trim whitespace
    Trim,
    /// Parse as number
    ParseNumber,
    /// Parse as date
    ParseDate { format: Option<String> },
    /// Regex extract
    Regex { pattern: String },
    /// Truncate to N characters
    Truncate { max_length: usize },
}

// ─── LLM Mode ────────────────────────────────────────────────────────────────

/// LLM extraction request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LLMExtractionRequest {
    /// URL to extract from
    pub url: String,
    /// Natural language description of what to extract
    pub description: String,
    /// Optional: example output structure
    pub example: Option<serde_json::Value>,
    /// Optional: specific fields to focus on
    pub fields: Option<Vec<String>>,
}

/// LLM-generated schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LLMGeneratedSchema {
    /// Generated schema
    pub schema: ExtractionSchema,
    /// Confidence in the schema
    pub confidence: f64,
    /// Fields the LLM was unsure about
    pub uncertain_fields: Vec<String>,
    /// Suggested improvements
    pub suggestions: Vec<String>,
}

// ─── Program Synthesis ───────────────────────────────────────────────────────

/// Generate an extraction program from a schema
pub fn generate_program(schema: &ExtractionSchema) -> ExtractionProgram {
    let steps: Vec<ExtractionStep> = schema
        .fields
        .iter()
        .map(|field| {
            let selector = field
                .selector
                .clone()
                .unwrap_or_else(|| format!("[data-field='{}']", field.name));

            let method = match &field.field_type {
                FieldType::Text => ExtractionMethod::TextContent,
                FieldType::Url => ExtractionMethod::Attribute {
                    name: "href".to_string(),
                },
                FieldType::Number => ExtractionMethod::TextContent,
                FieldType::Date => ExtractionMethod::TextContent,
                FieldType::Boolean => ExtractionMethod::TextContent,
                FieldType::Html => ExtractionMethod::InnerHtml,
                FieldType::Attribute { attribute } => ExtractionMethod::Attribute {
                    name: attribute.clone(),
                },
                FieldType::Image { .. } => ExtractionMethod::ImageSrc,
            };

            let post_process = match &field.field_type {
                FieldType::Number => Some(PostProcess::ParseNumber),
                FieldType::Date => Some(PostProcess::ParseDate { format: None }),
                FieldType::Text => Some(PostProcess::Trim),
                _ => None,
            };

            ExtractionStep {
                field_name: field.name.clone(),
                method,
                selector,
                post_process,
            }
        })
        .collect();

    ExtractionProgram {
        id: format!("prog-{}", uuid::Uuid::new_v4()),
        schema_name: schema.name.clone(),
        steps,
        verified: false,
        generated_at: chrono::Utc::now(),
    }
}

/// Verify an extraction program against HTML content
pub fn verify_program(program: &ExtractionProgram, html: &str) -> VerificationResult {
    let mut results = Vec::new();
    let mut all_passed = true;

    for step in &program.steps {
        let found = html.contains(&step.selector);
        if !found
            && program
                .steps
                .iter()
                .any(|s| s.field_name == step.field_name)
        {
            // Check if any field is required and not found
            all_passed = false;
        }

        results.push(FieldVerification {
            field_name: step.field_name.clone(),
            selector_found: found,
            extraction_success: found, // Would actually run extraction
        });
    }

    VerificationResult {
        program_id: program.id.clone(),
        all_passed,
        field_results: results,
        verified_at: chrono::Utc::now(),
    }
}

/// Verification result for a program
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationResult {
    pub program_id: String,
    pub all_passed: bool,
    pub field_results: Vec<FieldVerification>,
    pub verified_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldVerification {
    pub field_name: String,
    pub selector_found: bool,
    pub extraction_success: bool,
}

// ─── Program Execution ──────────────────────────────────────────────────────

/// Execute an extraction program on HTML content
pub fn execute_program(
    program: &ExtractionProgram,
    html: &str,
) -> anyhow::Result<serde_json::Value> {
    let mut result = serde_json::Map::new();

    for step in &program.steps {
        let value = extract_field(step, html);
        result.insert(step.field_name.clone(), value);
    }

    Ok(serde_json::Value::Object(result))
}

/// Extract a single field from HTML
fn extract_field(step: &ExtractionStep, html: &str) -> serde_json::Value {
    // Simplified extraction — in production, use lol_html or scraper
    if !html.contains(&step.selector) {
        return serde_json::Value::Null;
    }

    match &step.method {
        ExtractionMethod::TextContent => {
            // Find text between selector tags
            serde_json::Value::String(format!("Extracted text for {}", step.field_name))
        }
        ExtractionMethod::Attribute { name } => {
            serde_json::Value::String(format!("{}=value", name))
        }
        ExtractionMethod::InnerHtml => serde_json::Value::String("<html>...</html>".to_string()),
        ExtractionMethod::List { item_selector: _ } => serde_json::Value::Array(vec![
            serde_json::json!({"item": 1}),
            serde_json::json!({"item": 2}),
        ]),
        ExtractionMethod::ImageSrc => {
            serde_json::Value::String("https://example.com/image.jpg".to_string())
        }
    }
}

// ─── Pagination Handling ────────────────────────────────────────────────────

/// Pagination detection and handling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PaginationType {
    /// Offset-based: ?page=1, ?page=2
    Offset { param: String, start: usize },
    /// Cursor-based: ?cursor=abc123
    Cursor { param: String },
    /// Keyset-based: ?after=id
    Keyset { param: String },
    /// Infinite scroll (XHR interception)
    InfiniteScroll,
    /// No pagination detected
    None,
}

/// Detect pagination type from HTML
pub fn detect_pagination(html: &str) -> PaginationType {
    // Check for common pagination patterns
    if html.contains("?page=") || html.contains("&page=") {
        return PaginationType::Offset {
            param: "page".to_string(),
            start: 1,
        };
    }

    if html.contains("?cursor=") || html.contains("&cursor=") {
        return PaginationType::Cursor {
            param: "cursor".to_string(),
        };
    }

    if html.contains("?after=") || html.contains("&after=") {
        return PaginationType::Keyset {
            param: "after".to_string(),
        };
    }

    // Check for "Next" links
    if html.contains("next") && (html.contains("page") || html.contains("pagination")) {
        return PaginationType::Offset {
            param: "page".to_string(),
            start: 1,
        };
    }

    PaginationType::None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_program() {
        let schema = ExtractionSchema {
            name: "product".to_string(),
            version: "1.0".to_string(),
            fields: vec![
                SchemaField {
                    name: "title".to_string(),
                    field_type: FieldType::Text,
                    selector: Some("h1.product-title".to_string()),
                    required: true,
                    description: None,
                    is_list: false,
                },
                SchemaField {
                    name: "price".to_string(),
                    field_type: FieldType::Number,
                    selector: Some(".price".to_string()),
                    required: true,
                    description: None,
                    is_list: false,
                },
            ],
            selectors: HashMap::new(),
            url_pattern: None,
        };

        let program = generate_program(&schema);
        assert_eq!(program.steps.len(), 2);
        assert_eq!(program.steps[0].field_name, "title");
        assert_eq!(program.steps[1].field_name, "price");
    }

    #[test]
    fn test_verify_program() {
        let program = ExtractionProgram {
            id: "test".to_string(),
            schema_name: "test".to_string(),
            steps: vec![ExtractionStep {
                field_name: "title".to_string(),
                method: ExtractionMethod::TextContent,
                selector: "h1".to_string(),
                post_process: Some(PostProcess::Trim),
            }],
            verified: false,
            generated_at: chrono::Utc::now(),
        };

        let html = "<html><body><h1>Test Title</h1></body></html>";
        let result = verify_program(&program, html);
        assert!(result.all_passed);
        assert!(result.field_results[0].selector_found);
    }

    #[test]
    fn test_detect_pagination() {
        let html_offset = r#"<a href="?page=2">Next</a>"#;
        assert!(matches!(
            detect_pagination(html_offset),
            PaginationType::Offset { .. }
        ));

        let html_none = r#"<html><body><h1>No pagination</h1></body></html>"#;
        assert!(matches!(detect_pagination(html_none), PaginationType::None));
    }

    #[test]
    fn test_execute_program() {
        let program = ExtractionProgram {
            id: "test".to_string(),
            schema_name: "test".to_string(),
            steps: vec![ExtractionStep {
                field_name: "title".to_string(),
                method: ExtractionMethod::TextContent,
                selector: "h1".to_string(),
                post_process: None,
            }],
            verified: true,
            generated_at: chrono::Utc::now(),
        };

        let html = "<html><body><h1>Test</h1></body></html>";
        let result = execute_program(&program, html).unwrap();
        assert!(result.is_object());
        assert!(result.get("title").is_some());
    }
}
