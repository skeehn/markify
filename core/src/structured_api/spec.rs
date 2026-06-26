//! API spec types for structured API generation.

use serde::{Deserialize, Serialize};

/// A generated structured API specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiSpec {
    /// Unique API ID
    pub id: String,
    /// Source URL
    pub url: String,
    /// User's description of what they want
    pub description: Option<String>,
    /// Generated name for this API
    pub name: String,
    /// List of endpoints in this API
    pub endpoints: Vec<Endpoint>,
    /// JSON Schema for the response
    pub response_schema: serde_json::Value,
    /// OpenAPI 3.1 spec (generated)
    pub openapi_spec: Option<serde_json::Value>,
    /// MCP tool definition
    pub mcp_tool: Option<serde_json::Value>,
    /// When this API was generated
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Status
    pub status: ApiStatus,
}

/// A single endpoint in an API spec
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Endpoint {
    /// Endpoint name (e.g., "list_articles", "get_product")
    pub name: String,
    /// Description of what this endpoint extracts
    pub description: String,
    /// For list endpoints: a CSS selector matching each repeating item. Field
    /// rules are then applied *relative to* each matched container element.
    /// `None` for single-object endpoints (rules are page-absolute).
    #[serde(default)]
    pub container_selector: Option<String>,
    /// CSS selectors or extraction rules
    pub extraction_rules: Vec<ExtractionRule>,
    /// Optional CSS selector for the "next page" link. When set and `max_pages`
    /// > 1 at execution time, the executor follows it and aggregates results
    /// across pages.
    #[serde(default)]
    pub next_page_selector: Option<String>,
    /// Expected output type
    pub output_type: OutputType,
    /// Whether this returns a list or a single object
    pub returns_list: bool,
}

/// A single extraction rule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractionRule {
    /// Field name in output
    pub field: String,
    /// CSS selector to extract from
    pub selector: String,
    /// What to extract: "text", "href", "src", "html", "attr:name"
    pub extract: String,
    /// Whether this field is required
    pub required: bool,
    /// Output type for this field
    pub field_type: FieldType,
}

/// Output type for an endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputType {
    /// Single object (e.g., one article)
    Object,
    /// List of objects (e.g., list of products)
    List,
}

/// Field type for extraction
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldType {
    String,
    Number,
    Boolean,
    Url,
    Date,
    List,
}

/// API status
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApiStatus {
    /// API was generated and is ready to use
    Completed,
    /// API generation is in progress
    Generating,
    /// API generation failed
    Failed,
}

/// Result of executing an API spec against a URL
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    /// API spec ID
    pub api_id: String,
    /// Endpoint that was executed
    pub endpoint: String,
    /// Extracted data
    pub data: Vec<serde_json::Value>,
    /// Execution time in ms
    pub execution_ms: u64,
    /// Source URL used for this execution
    pub source_url: String,
}

/// Request to generate an API spec
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerateApiRequest {
    /// URL to analyze
    pub url: String,
    /// Natural language description of the data to extract
    pub description: Option<String>,
}

/// Request to execute an API spec
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecuteApiRequest {
    /// Override URL (defaults to original spec URL)
    pub url: Option<String>,
    /// Input parameters (for parameterized endpoints)
    pub params: Option<serde_json::Value>,
    /// Maximum number of pages to follow via the endpoint's `next_page_selector`
    /// (default 1 = current page only; capped at 20).
    #[serde(default)]
    pub max_pages: Option<usize>,
}
