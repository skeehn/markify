//! Structured JSON output generation.

use serde_json::json;

use crate::extract::{ImageInfo, LinkInfo, Metadata};

/// Build structured JSON from extracted content and metadata.
pub fn to_structured_json(
    title: Option<String>,
    content: Option<String>,
    metadata: Option<Metadata>,
    links: Option<Vec<LinkInfo>>,
    images: Option<Vec<ImageInfo>>,
) -> serde_json::Value {
    let mut result = json!({});

    if let Some(t) = title {
        result["title"] = json!(t);
    }

    if let Some(c) = content {
        result["content"] = json!(c);
    }

    // Add metadata
    if let Some(meta) = metadata {
        result["metadata"] = json!({
            "description": meta.description,
            "image": meta.image,
            "author": meta.author,
            "published_date": meta.published_date,
            "language": meta.language,
            "canonical_url": meta.canonical_url,
            "site_name": meta.site_name,
            "schema_org": meta.schema_org,
        });
    }

    // Add links
    if let Some(links) = links {
        result["links"] = json!(links
            .iter()
            .map(|l| {
                json!({
                    "text": l.text,
                    "url": l.url,
                    "score": l.score,
                    "is_internal": l.is_internal,
                })
            })
            .collect::<Vec<_>>());
    }

    // Add images
    if let Some(images) = images {
        result["images"] = json!(images
            .iter()
            .map(|i| {
                json!({
                    "src": i.src,
                    "alt": i.alt,
                    "width": i.width,
                    "height": i.height,
                    "is_content": i.is_content,
                })
            })
            .collect::<Vec<_>>());
    }

    result
}
