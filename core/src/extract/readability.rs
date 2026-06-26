//! Article extraction using dom_smoothie (Mozilla Readability port).
//!
//! This is the ONLY Readability implementation in Rust that consistently
//! isolates main article content across diverse page structures.

use dom_smoothie::{Config, Readability};
use tracing::{debug, warn};

use crate::extract::ExtractedContent;

/// Extract the main article content from an HTML document.
/// Strips navigation, ads, sidebars, and other boilerplate.
pub fn extract_article(html: &str) -> Option<ExtractedContent> {
    let config = Config::default();
    let mut readability = match Readability::new(html, None, Some(config)) {
        Ok(r) => r,
        Err(e) => {
            warn!(error = %e, "Failed to create Readability instance");
            return None;
        }
    };

    match readability.parse() {
        Ok(article) => {
            debug!(
                title = ?article.title,
                byline = ?article.byline,
                length = article.text_content.len(),
                "Article extracted successfully"
            );

            // text_content is the clean article prose. Strip "[edit]" section
            // markers that wikis (e.g. Wikipedia) leak into the extracted text.
            let content = article.text_content.to_string().replace("[edit]", "");
            let title = article.title.clone();
            let byline = article.byline.clone();
            let excerpt = article.excerpt.clone();
            let site_name = article.site_name.clone();

            Some(ExtractedContent {
                content: Some(content),
                structured: Some(serde_json::json!({
                    "title": title,
                    "byline": byline,
                    "excerpt": excerpt,
                    "site_name": site_name,
                })),
                metadata: None,
                links: None,
                images: None,
            })
        }
        Err(e) => {
            warn!(error = %e, "Article extraction failed");
            None
        }
    }
}
