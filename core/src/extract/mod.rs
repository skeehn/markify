//! Content extraction module.
//!
//! Supports multiple extraction strategies:
//! - Article extraction via Readability (dom_smoothie)
//! - Streaming HTML cleanup via lol_html
//! - Metadata extraction (OG, Twitter Cards, JSON-LD)
//! - Link extraction with relevance scoring

pub mod links;
pub mod metadata;
pub mod readability;
pub mod streaming;

pub use links::extract_links;
pub use metadata::extract_metadata;
pub use readability::extract_article;
pub use streaming::clean_html_to_markdown;

use serde::{Deserialize, Serialize};

/// Extraction mode
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtractionMode {
    /// Extract main article content only (strips nav, ads, sidebars)
    Article,
    /// Full page conversion
    Full,
    /// Extract links only
    Links,
    /// Extract images only
    Images,
    /// Extract metadata only (OG tags, title, description)
    Metadata,
    /// Auto-detect content type and pick best strategy
    #[default]
    Smart,
}

/// Extracted content result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedContent {
    /// The main content (Markdown)
    pub content: Option<String>,
    /// Structured JSON content
    pub structured: Option<serde_json::Value>,
    /// Extracted metadata
    pub metadata: Option<Metadata>,
    /// Extracted links
    pub links: Option<Vec<LinkInfo>>,
    /// Extracted images
    pub images: Option<Vec<ImageInfo>>,
}

/// Page metadata
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Metadata {
    pub title: Option<String>,
    pub description: Option<String>,
    pub image: Option<String>,
    pub url: Option<String>,
    pub site_name: Option<String>,
    pub author: Option<String>,
    pub published_date: Option<String>,
    pub language: Option<String>,
    pub canonical_url: Option<String>,
    /// Open Graph type
    pub og_type: Option<String>,
    /// Twitter card type
    pub twitter_card: Option<String>,
    /// JSON-LD structured data
    pub schema_org: Option<serde_json::Value>,
    /// Favicon
    pub favicon: Option<String>,
}

/// Extracted link with relevance score
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkInfo {
    pub text: String,
    pub url: String,
    /// Relevance score 0.0-1.0 (based on position, context, text)
    pub score: f64,
    /// Is this an internal link?
    pub is_internal: bool,
}

/// Extracted image info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageInfo {
    pub src: String,
    pub alt: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// Is this likely a decorative/non-content image?
    pub is_content: bool,
}

/// Detection result for smart mode
#[derive(Debug, Clone)]
pub struct ContentDetection {
    /// Is this an article/blog post?
    pub is_article: bool,
    /// Is this a product page?
    pub is_product: bool,
    /// Is this a listing/index page?
    pub is_listing: bool,
    /// Is this a documentation page?
    pub is_docs: bool,
    /// Confidence 0.0-1.0
    pub confidence: f64,
}
