//! Markdown transformation using html2md (fast_html2md).

/// Convert HTML to Markdown.
/// Uses html2md built on Cloudflare's lol_html for streaming,
/// maintaining constant 5-6KB memory regardless of input size.
pub fn to_markdown(html: &str) -> String {
    html2md::rewrite_html(html, true)
}

/// Convert HTML to Markdown with custom options.
pub fn to_markdown_with_options(html: &str, _strip_links: bool, _strip_images: bool) -> String {
    html2md::rewrite_html(html, true)
}
