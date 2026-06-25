//! Transform module: output generation.
//!
//! Produces both Markdown and structured JSON from extracted content.

pub mod json;
pub mod markdown;

pub use json::to_structured_json;
pub use markdown::to_markdown;

use serde::{Deserialize, Serialize};

/// Output format selection
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum OutputFormat {
    /// Markdown only
    Markdown,
    /// Structured JSON only
    Json,
    /// Both Markdown and JSON
    #[default]
    Both,
}

/// Combined output result
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TransformResult {
    /// Markdown output (if requested)
    pub markdown: Option<String>,
    /// Structured JSON output (if requested)
    pub json: Option<serde_json::Value>,
}

impl TransformResult {
    pub fn markdown_only(md: String) -> Self {
        Self {
            markdown: Some(md),
            json: None,
        }
    }

    pub fn json_only(json: serde_json::Value) -> Self {
        Self {
            markdown: None,
            json: Some(json),
        }
    }

    pub fn both(md: String, json: serde_json::Value) -> Self {
        Self {
            markdown: Some(md),
            json: Some(json),
        }
    }
}
