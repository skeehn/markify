//! VSB-Graph emitters — export blocks to clean Markdown and structured JSON.
//!
//! Replaces flat html2md conversion with structured block-based output.

use crate::vsb_graph::types::*;

/// Emit a VSB-Graph to clean Markdown, respecting block structure.
pub fn emit_blocks(graph: &VSBGraph, format: OutputFormat) -> String {
    match format {
        OutputFormat::Markdown => emit_markdown(graph),
        OutputFormat::Json => emit_json(graph),
        OutputFormat::Both => emit_markdown(graph),
    }
}

/// Output format for VSB-Graph emission
#[derive(Debug, Clone)]
pub enum OutputFormat {
    Markdown,
    Json,
    Both,
}

/// Emit to clean Markdown with block-level provenance.
pub fn emit_markdown(graph: &VSBGraph) -> String {
    let mut md = String::new();

    // Page header
    if let Some(title) = &graph.page_title {
        md.push_str(&format!("# {}\n\n", title));
    }

    if let Some(lang) = &graph.page_language {
        md.push_str(&format!("> Language: {}\n\n", lang));
    }

    // Content blocks in document order
    for block_id in &graph.roots {
        if let Some(block) = graph.blocks.get(block_id) {
            if !block.is_boilerplate {
                render_block(block, graph, &mut md, 0);
            }
        }
    }

    md
}

/// Render a single block to Markdown recursively.
fn render_block(block: &VSBBlock, graph: &VSBGraph, md: &mut String, depth: usize) {
    match block.block_type {
        BlockType::Article
        | BlockType::BlogPost
        | BlockType::Documentation
        | BlockType::Tutorial
        | BlockType::Specification
        | BlockType::Changelog
        | BlockType::ProductDetail
        | BlockType::ProductListing
        | BlockType::FeatureGrid
        | BlockType::Feed
        | BlockType::Forum
        | BlockType::Generic => {
            if !block.text.is_empty() {
                md.push_str(&block.text);
                md.push_str("\n\n");
            }
        }
        BlockType::FAQ => {
            if !block.text.is_empty() {
                md.push_str("### FAQ\n\n");
                md.push_str(&block.text);
                md.push_str("\n\n");
            }
        }
        BlockType::Table => {
            if !block.text.is_empty() {
                md.push_str(&block.text);
                md.push_str("\n\n");
            }
        }
        BlockType::Code | BlockType::APIDocumentation => {
            if !block.text.is_empty() {
                md.push_str("```\n");
                md.push_str(&block.text);
                md.push_str("\n```\n\n");
            }
        }
        BlockType::Chart => {
            if !block.text.is_empty() {
                md.push_str(&format!("[Chart: {}]\n\n", block.text));
            }
        }
        BlockType::Media => {
            if !block.text.is_empty() {
                md.push_str(&format!("[Media: {}]\n\n", block.text));
            }
        }
        BlockType::Hero => {
            if !block.text.is_empty() && depth <= 1 {
                md.push_str(&block.text);
                md.push_str("\n\n");
            }
        }
        BlockType::CallToAction => {
            if !block.text.is_empty() {
                md.push_str(&format!("> **{}**\n\n", block.text));
            }
        }
        BlockType::Testimonial => {
            if !block.text.is_empty() {
                md.push_str(&format!("> \"{}\"\n\n", block.text));
            }
        }
        BlockType::Pricing => {
            if !block.text.is_empty() {
                md.push_str("### Pricing\n\n");
                md.push_str(&block.text);
                md.push_str("\n\n");
            }
        }
        BlockType::Review => {
            if !block.text.is_empty() {
                md.push_str("### Reviews\n\n");
                md.push_str(&block.text);
                md.push_str("\n\n");
            }
        }
        BlockType::Cart => {
            if !block.text.is_empty() {
                md.push_str("### Cart\n\n");
                md.push_str(&block.text);
                md.push_str("\n\n");
            }
        }
        BlockType::ProductCard => {
            if !block.text.is_empty() {
                md.push_str(&block.text);
                md.push_str("\n\n");
            }
        }
        BlockType::UserProfile => {
            if !block.text.is_empty() {
                md.push_str(&format!("**{}**\n\n", block.text));
            }
        }
        BlockType::Comment => {
            if !block.text.is_empty() {
                md.push_str(&format!("> {}\n\n", block.text));
            }
        }
        BlockType::Navigation
        | BlockType::Advertisement
        | BlockType::Breadcrumb
        | BlockType::Pagination
        | BlockType::TableOfContents => {
            // Skip boilerplate navigation types
        }
        BlockType::Header => {
            if !block.text.is_empty() && depth == 0 {
                md.push_str(&format!("## {}\n\n", block.text));
            }
        }
        BlockType::Footer => {
            // Skip footers
        }
        BlockType::Sidebar => {
            // Skip sidebars unless explicitly requested
        }
        BlockType::Search | BlockType::Form | BlockType::Login | BlockType::Filter => {
            // Interactive elements — render summary only
            if !block.text.is_empty() {
                md.push_str(&format!("[{}]\n\n", block.block_type.block_type_label()));
            }
        }
    }

    // Render children
    for child_id in &block.children {
        if let Some(child) = graph.blocks.get(child_id) {
            if !child.is_boilerplate {
                render_block(child, graph, md, depth + 1);
            }
        }
    }
}

/// Emit to structured JSON with full block detail.
pub fn emit_json(graph: &VSBGraph) -> String {
    serde_json::to_string_pretty(&graph.to_json()).unwrap_or_default()
}
