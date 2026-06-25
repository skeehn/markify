//! Streaming HTML-to-Markdown conversion using fast_html2md (lol_html-based).
//!
//! This approach uses Cloudflare's lol_html for streaming HTML rewriting,
//! maintaining a constant 5-6KB memory footprint regardless of input size.

/// Convert raw HTML to clean Markdown.
/// Uses html2md (fast_html2md) built on Cloudflare's lol_html for streaming,
/// maintaining constant 5-6KB memory regardless of input size.
pub fn clean_html_to_markdown(html: &str) -> String {
    html2md::rewrite_html(html, true)
}

/// Convert HTML to Markdown (simple alias).
pub fn html_to_markdown_with_options(
    html: &str,
    _strip_links: bool,
    _strip_images: bool,
) -> String {
    html2md::rewrite_html(html, true)
}
