//! VSB-Graph core types — stable, versioned content blocks with provenance.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Unique identifier for a content block
pub type BlockId = String;

/// A Visual-Semantic Block — the fundamental unit of the VSB-Graph.
/// Each block represents a layout-aware, semantically-coherent content region.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VSBBlock {
    /// Stable, versioned block ID (persists across page updates if content is semantically same)
    pub id: BlockId,
    /// Content fingerprint (for dedup/version detection)
    pub content_hash: String,
    /// Block type classification
    pub block_type: BlockType,
    /// Semantic role of this block
    pub semantic_role: SemanticRole,
    /// Clean text content
    pub text: String,
    /// HTML structure (optional, for provenance)
    pub html_fragment: Option<String>,
    /// CSS selectors that identify this block's source
    pub source_selectors: Vec<String>,
    /// Block position hints (for visual reconstruction)
    pub position: Option<BlockPosition>,
    /// Links found within this block
    pub links: Vec<BlockLink>,
    /// Images found within this block
    pub images: Vec<BlockImage>,
    /// Provenance: where this block came from
    pub provenance: Provenance,
    /// Version number (increments when block changes)
    pub version: u32,
    /// Whether this block is boilerplate (nav, footer, ads, etc.)
    pub is_boilerplate: bool,
    /// Boilerplate confidence 0.0-1.0
    pub boilerplate_score: f64,
    /// Child blocks (for nested structure)
    pub children: Vec<BlockId>,
    /// Parent block ID
    pub parent: Option<BlockId>,
    /// Metadata (arbitrary key-value pairs)
    pub metadata: HashMap<String, String>,
}

/// Type of content block — a comprehensive taxonomy covering all common web page patterns.
///
/// Organized into six families:
///   - Document: long-form, structured text content
///   - UI Component: interactive navigation and input widgets
///   - Commerce: e-commerce and transactional elements
///   - Data: tabular, visual, or programmatic information
///   - Social: user-generated and community content
///   - Structural: layout-level presentation blocks
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum BlockType {
    // ─── Document family ───
    /// Long-form article body (news, magazine, journalistic content).
    /// Identifiers: <article>, .article, .post, .entry-content, .mw-parser-output,
    /// high text density, multiple paragraphs, byline/date metadata nearby.
    Article,

    /// Blog post — similar to Article but with stronger social/engagement signals.
    /// Identifiers: .blog-post, .single-post, author avatar, tags/categories,
    /// reading-time estimate, share buttons adjacent.
    BlogPost,

    /// Technical or product documentation page/section.
    /// Identifiers: .docs, .documentation, .api-doc, sidebar TOC,
    /// version selectors, "on this page" navigation, code samples intermixed.
    Documentation,

    /// FAQ block — question/answer pairs.
    /// Identifiers: FAQPage schema, <details>/<summary>, .faq, .q-and-a,
    /// accordion patterns, "Frequently Asked Questions" heading.
    FAQ,

    /// Technical specification or standards document.
    /// Identifiers: RFC-style numbering, .spec, .standard, normative language
    /// ("MUST", "SHOULD"), section cross-references, table of contents.
    Specification,

    /// Changelog / release notes — chronological list of version changes.
    /// Identifiers: .changelog, .release-notes, semantic version headings,
    /// date-stamped entries, "Added/Fixed/Changed" categories.
    Changelog,

    /// How-to guide or tutorial — step-by-step instructional content.
    /// Identifiers: numbered steps, .tutorial, .how-to, "Step 1/2/3",
    /// code blocks intermixed with explanatory text, completion checkmarks.
    Tutorial,

    // ─── UI Component family ───
    /// Navigation element — menus, nav bars, tab bars.
    /// Identifiers: <nav>, .nav, .navigation, .menu, ul > li > a patterns,
    /// "home" link present, horizontal or vertical link lists.
    Navigation,

    /// Header/banner region — top-of-page branding and primary nav container.
    /// Identifiers: <header>, .header, .banner, .topbar, .navbar, logo image,
    /// site title, positioned at top of viewport.
    Header,

    /// Footer region — bottom-of-page links, legal, copyright.
    /// Identifiers: <footer>, .footer, .bottombar, "Privacy Policy", "Terms",
    /// copyright notice, sitemap links, positioned at page bottom.
    Footer,

    /// Sidebar/aside — supplementary content alongside primary content.
    /// Identifiers: <aside>, .sidebar, .aside, .widget-area, positioned left
    /// or right of main content, contains TOC, related links, or widgets.
    Sidebar,

    /// Search interface — search box, search results, or autocomplete dropdown.
    /// Identifiers: <form> with search input, .search, .search-box,
    /// magnifying glass icon, type="search", autocomplete suggestions.
    Search,

    /// Form — data input interface (contact, signup, settings, multi-step).
    /// Identifiers: <form>, input/select/textarea children, .form,
    /// submit button, validation messages, field labels.
    Form,

    /// Login / authentication interface.
    /// Identifiers: .login, .signin, .auth, "Sign in"/"Log in" heading,
    /// username + password fields, OAuth provider buttons, "Forgot password".
    Login,

    /// Filter controls — faceted search, category filters, sort dropdowns.
    /// Identifiers: .filters, .facets, .filter-bar, checkbox/radio groups,
    /// price range sliders, "Sort by" dropdowns, active filter chips.
    Filter,

    /// Pagination — page navigation controls.
    /// Identifiers: .pagination, numbered page links, "Next/Previous" buttons,
    /// "Page X of Y", ellipsis for skipped pages, <nav> with aria-label="Pagination".
    Pagination,

    /// Breadcrumb trail — hierarchical page location path.
    /// Identifiers: .breadcrumb, <nav aria-label="breadcrumb">,
    /// separator characters (/  > ), linked path segments, Schema BreadcrumbList.
    Breadcrumb,

    /// Table of contents — structured outline of page sections.
    /// Identifiers: .toc, .table-of-contents, #toc, ordered/unordered list
    /// of anchor links matching page headings, collapsible/expandable.
    TableOfContents,

    // ─── Commerce family ───
    /// Product card — compact item in a listing/grid (image + title + price).
    /// Identifiers: .product-card, .product-item, .card, thumbnail image,
    /// product title, price, "Add to cart" or "View" button, rating stars.
    ProductCard,

    /// Product listing page (PLP) — grid/list of product cards.
    /// Identifiers: .product-list, .product-grid, .category-page,
    /// multiple ProductCard children, filter sidebar, sort controls,
    /// result count ("Showing 1-24 of 156").
    ProductListing,

    /// Product detail page (PDP) — single product full information.
    /// Identifiers: .product-detail, .product-page, image gallery,
    /// size/color selectors, quantity picker, detailed description,
    /// specifications table, reviews section.
    ProductDetail,

    /// Pricing information — price display, comparison, or pricing table.
    /// Identifiers: .price, .pricing, .pricing-table, currency symbols,
    /// strike-through original price, discount badges, plan comparison rows.
    Pricing,

    /// Reviews and ratings — user reviews with star ratings.
    /// Identifiers: .reviews, .rating, Review/AggregateRating schema,
    /// star icons, reviewer names, dates, "X out of 5" scores,
    /// verified purchase badges.
    Review,

    /// Shopping cart — items selected for purchase.
    /// Identifiers: .cart, .basket, .shopping-cart, item rows with
    /// quantity controls, subtotal/total, "Checkout" button,
    /// remove item links, promo code input.
    Cart,

    // ─── Data family ───
    /// Table — structured row/column data.
    /// Identifiers: <table>, .data-table, <thead>/<tbody>, th/td cells,
    /// sortable column headers, striped rows, pagination within table.
    Table,

    /// Chart or data visualization — graphs, maps, dashboards.
    /// Identifiers: <canvas>, <svg> charts, .chart, .graph, .visualization,
    /// Chart.js/D3.js/Plotly containers, axis labels, legend, tooltips.
    Chart,

    /// Code block — syntax-highlighted source code or terminal output.
    /// Identifiers: <pre><code>, .code-block, .highlight, Prism/Highlight.js
    /// classes, line numbers, copy button, language label, monospace font.
    Code,

    /// API documentation — endpoint specs, request/response examples.
    /// Identifiers: .api-doc, .endpoint, HTTP method badges (GET/POST),
    /// URL paths, JSON request/response bodies, parameter tables,
    /// "Try it" console, OpenAPI/Swagger UI elements.
    APIDocumentation,

    // ─── Social family ───
    /// Comment thread — user-generated discussion.
    /// Identifiers: .comment, .comments, .discussion, nested reply threads,
    /// author avatars, timestamps, "Reply" buttons, upvote/downvote,
    /// Comment schema markup.
    Comment,

    /// User profile — identity card or profile section.
    /// Identifiers: .profile, .user-card, .author-box, avatar image,
    /// display name, bio text, social links, follower counts,
    /// "Edit profile" or "Follow" buttons.
    UserProfile,

    /// Social feed — chronological stream of posts/updates.
    /// Identifiers: .feed, .timeline, .stream, infinite scroll,
    /// like/retweet/reply actions, timestamps ("2h ago"),
    /// user avatars per item, engagement counts.
    Feed,

    /// Forum thread — structured discussion board topic.
    /// Identifiers: .forum, .thread, .topic, post author columns,
    /// user reputation/badges, "Last post" timestamps, page navigation,
    /// quote blocks, signature sections.
    Forum,

    // ─── Structural family ───
    /// Hero section — large above-the-fold banner with key message/CTA.
    /// Identifiers: .hero, .hero-banner, .jumbotron, full-width background
    /// image/video, large heading, prominent CTA button, positioned
    /// at top of page below header.
    Hero,

    /// Feature grid — structured showcase of product features or benefits.
    /// Identifiers: .features, .feature-grid, .benefits, icon + heading +
    /// description cards in grid layout, 2-4 columns, alternating image/text
    /// rows, checkmark lists.
    FeatureGrid,

    /// Testimonial — customer quote or endorsement with attribution.
    /// Identifiers: .testimonial, .quote, .customer-story, quotation marks,
    /// customer name/title/company, star rating, customer photo,
    /// carousel/slider wrapper.
    Testimonial,

    /// Call-to-action (CTA) — conversion-focused prompt block.
    /// Identifiers: .cta, .call-to-action, .signup-prompt, prominent button
    /// ("Get Started", "Sign Up Free", "Subscribe"), urgency language,
    /// contrasting background color, email input field.
    CallToAction,

    /// Media embed — video, audio, podcast, or iframe content.
    /// Identifiers: <video>, <audio>, <iframe> (YouTube/Vimeo/Spotify),
    /// .video-embed, .media-player, play button overlay, duration,
    /// thumbnail/poster image.
    Media,

    /// Advertisement — sponsored or promotional content.
    /// Identifiers: .ad, .ads, .advert, .sponsored, .promoted, "Ad" label,
    /// Google AdSense / DFP containers, IAB standard sizes (300x250, 728x90),
    /// "Sponsored by" text, affiliate disclosure.
    Advertisement,

    /// Generic / unknown — unclassifiable content block.
    /// Used as fallback when no specific type matches.
    Generic,
}

/// Semantic role — what this block means in the context of the page.
///
/// Roles are orthogonal to block types: the same `BlockType` can play different
/// roles depending on placement. For example, a `Table` in the main content area
/// is `PrimaryContent`, while the same table in a sidebar is `SupportingContent`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SemanticRole {
    /// Primary content — the main reason the user is on this page.
    /// Examples: article body, product detail, search results.
    PrimaryContent,

    /// Supporting content — related material that enriches primary content.
    /// Examples: related posts, tags, sidebar widgets, author bio.
    SupportingContent,

    /// Navigation — wayfinding and site structure.
    /// Examples: main menu, breadcrumbs, pagination, TOC.
    Navigation,

    /// Meta — information about the content rather than the content itself.
    /// Examples: author, publish date, reading time, category tags, word count.
    Meta,

    /// Interactive — elements that accept user input or trigger actions.
    /// Examples: forms, search boxes, filters, login, cart controls.
    Interactive,

    /// Structural — layout and presentation scaffolding.
    /// Examples: header, footer, dividers, whitespace blocks.
    Structural,

    /// Commercial — revenue-generating or promotional content.
    /// Examples: ads, sponsored content, pricing tables, affiliate links.
    Commercial,

    /// User-generated — content created by users rather than the site owner.
    /// Examples: comments, reviews, forum posts, feed items.
    UserGenerated,

    /// Unknown — role could not be determined.
    Unknown,
}

impl BlockType {
    /// Return a human-readable label for this block type.
    pub fn block_type_label(&self) -> &'static str {
        match self {
            BlockType::Article => "Article",
            BlockType::BlogPost => "Blog Post",
            BlockType::Documentation => "Documentation",
            BlockType::FAQ => "FAQ",
            BlockType::Specification => "Specification",
            BlockType::Changelog => "Changelog",
            BlockType::Tutorial => "Tutorial",
            BlockType::Navigation => "Navigation",
            BlockType::Header => "Header",
            BlockType::Footer => "Footer",
            BlockType::Sidebar => "Sidebar",
            BlockType::Search => "Search",
            BlockType::Form => "Form",
            BlockType::Login => "Login",
            BlockType::Filter => "Filter",
            BlockType::Pagination => "Pagination",
            BlockType::Breadcrumb => "Breadcrumb",
            BlockType::TableOfContents => "Table of Contents",
            BlockType::ProductCard => "Product Card",
            BlockType::ProductListing => "Product Listing",
            BlockType::ProductDetail => "Product Detail",
            BlockType::Pricing => "Pricing",
            BlockType::Review => "Review",
            BlockType::Cart => "Cart",
            BlockType::Table => "Table",
            BlockType::Chart => "Chart",
            BlockType::Code => "Code",
            BlockType::APIDocumentation => "API Documentation",
            BlockType::Comment => "Comment",
            BlockType::UserProfile => "User Profile",
            BlockType::Feed => "Feed",
            BlockType::Forum => "Forum",
            BlockType::Hero => "Hero",
            BlockType::FeatureGrid => "Feature Grid",
            BlockType::Testimonial => "Testimonial",
            BlockType::CallToAction => "Call to Action",
            BlockType::Media => "Media",
            BlockType::Advertisement => "Advertisement",
            BlockType::Generic => "Content",
        }
    }
}

/// Block position hints from CSS layout analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockPosition {
    /// Approximate vertical order on page
    pub order: usize,
    /// Depth in DOM tree
    pub depth: usize,
    /// Whether this block is in the main content area
    pub is_central: bool,
}

/// Link within a block
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockLink {
    pub text: String,
    pub href: String,
    /// Whether this is an internal link
    pub is_internal: bool,
    /// Link relevance score 0.0-1.0
    pub relevance: f64,
}

/// Image within a block
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockImage {
    pub src: String,
    pub alt: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// Whether this is a content image (not decorative)
    pub is_content: bool,
}

/// Provenance: full audit trail for a block
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provenance {
    /// Source URL
    pub source_url: String,
    /// Timestamp when block was extracted
    pub extracted_at: chrono::DateTime<chrono::Utc>,
    /// Extraction engine used
    pub engine: String,
    /// Fetch time in milliseconds
    pub fetch_ms: u64,
    /// Processing time in milliseconds
    pub processing_ms: u64,
    /// CSS selector path to this block
    pub css_path: String,
    /// XPath to this block
    pub xpath: String,
}

/// The complete VSB-Graph for a page
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VSBGraph {
    /// All blocks in this graph
    pub blocks: HashMap<BlockId, VSBBlock>,
    /// Root block IDs (top-level blocks)
    pub roots: Vec<BlockId>,
    /// Page metadata
    pub page_url: String,
    /// Page title
    pub page_title: Option<String>,
    /// Page language
    pub page_language: Option<String>,
    /// Content fingerprint for the entire page
    pub page_hash: String,
    /// When this graph was created
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Total text content length
    pub total_text_length: usize,
    /// Number of content blocks (excluding boilerplate)
    pub content_block_count: usize,
    /// Number of boilerplate blocks
    pub boilerplate_block_count: usize,
}

impl VSBGraph {
    /// Get all non-boilerplate blocks in document order
    pub fn content_blocks(&self) -> Vec<&VSBBlock> {
        self.roots
            .iter()
            .filter_map(|id| self.blocks.get(id))
            .filter(|b| !b.is_boilerplate)
            .collect()
    }

    /// Get the primary content block(s)
    pub fn primary_content(&self) -> Vec<&VSBBlock> {
        self.blocks
            .values()
            .filter(|b| b.semantic_role == SemanticRole::PrimaryContent && !b.is_boilerplate)
            .collect()
    }

    /// Export to clean Markdown (for backward compatibility)
    pub fn to_markdown(&self) -> String {
        let mut md = String::new();

        if let Some(title) = &self.page_title {
            md.push_str(&format!("# {}\n\n", title));
        }

        for block_id in &self.roots {
            if let Some(block) = self.blocks.get(block_id) {
                if !block.is_boilerplate {
                    self.render_block_markdown(block, &mut md, 0);
                }
            }
        }

        md
    }

    fn render_block_markdown(&self, block: &VSBBlock, md: &mut String, depth: usize) {
        let indent = "  ".repeat(depth);

        match block.block_type {
            BlockType::Article
            | BlockType::BlogPost
            | BlockType::Documentation
            | BlockType::Tutorial
            | BlockType::Specification
            | BlockType::Changelog
            | BlockType::Generic => {
                if !block.text.is_empty() {
                    md.push_str(&format!("{}{}\n\n", indent, block.text));
                }
            }
            BlockType::FAQ => {
                if !block.text.is_empty() {
                    md.push_str(&format!("{}### FAQ\n\n{}\n\n", indent, block.text));
                }
            }
            BlockType::Table => {
                if !block.text.is_empty() {
                    md.push_str(&format!("{}{}\n\n", indent, block.text));
                }
            }
            BlockType::Code | BlockType::APIDocumentation => {
                if !block.text.is_empty() {
                    md.push_str(&format!("{}```\n{}\n```\n\n", indent, block.text));
                }
            }
            BlockType::Chart => {
                if !block.text.is_empty() {
                    md.push_str(&format!("{}*[Chart: {}]*\n\n", indent, block.text));
                }
            }
            BlockType::Media => {
                if !block.text.is_empty() {
                    md.push_str(&format!("{}*[Media: {}]*\n\n", indent, block.text));
                }
            }
            BlockType::Hero => {
                if !block.text.is_empty() {
                    md.push_str(&format!("{}\n\n{}\n\n", indent, block.text));
                }
            }
            BlockType::CallToAction => {
                if !block.text.is_empty() {
                    md.push_str(&format!("{}> **{}**\n\n", indent, block.text));
                }
            }
            BlockType::Testimonial => {
                if !block.text.is_empty() {
                    md.push_str(&format!("{}> \"{}\"\n\n", indent, block.text));
                }
            }
            BlockType::Pricing => {
                if !block.text.is_empty() {
                    md.push_str(&format!("{}### Pricing\n\n{}\n\n", indent, block.text));
                }
            }
            BlockType::Review => {
                if !block.text.is_empty() {
                    md.push_str(&format!("{}### Reviews\n\n{}\n\n", indent, block.text));
                }
            }
            BlockType::Cart => {
                if !block.text.is_empty() {
                    md.push_str(&format!("{}### Cart\n\n{}\n\n", indent, block.text));
                }
            }
            BlockType::ProductDetail => {
                if !block.text.is_empty() {
                    md.push_str(&format!("{}{}\n\n", indent, block.text));
                }
            }
            BlockType::ProductListing | BlockType::ProductCard | BlockType::FeatureGrid => {
                if !block.text.is_empty() {
                    md.push_str(&format!("{}{}\n\n", indent, block.text));
                }
            }
            BlockType::UserProfile => {
                if !block.text.is_empty() {
                    md.push_str(&format!("{}**{}**\n\n", indent, block.text));
                }
            }
            BlockType::Feed | BlockType::Forum => {
                if !block.text.is_empty() {
                    md.push_str(&format!("{}{}\n\n", indent, block.text));
                }
            }
            BlockType::Comment => {
                if !block.text.is_empty() {
                    md.push_str(&format!("{}> {}\n\n", indent, block.text));
                }
            }
            BlockType::Navigation
            | BlockType::Header
            | BlockType::Footer
            | BlockType::Sidebar
            | BlockType::Search
            | BlockType::Form
            | BlockType::Login
            | BlockType::Filter
            | BlockType::Pagination
            | BlockType::Breadcrumb
            | BlockType::TableOfContents
            | BlockType::Advertisement => {
                // These are typically boilerplate or structural; skip in content markdown
            }
        }

        // Render children (with depth limit to prevent infinite recursion)
        if depth < 20 {
            for child_id in &block.children {
                if let Some(child) = self.blocks.get(child_id) {
                    if !child.is_boilerplate {
                        self.render_block_markdown(child, md, depth + 1);
                    }
                }
            }
        }
    }

    /// Export to structured JSON
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "url": self.page_url,
            "title": self.page_title,
            "language": self.page_language,
            "blocks": self.blocks.values().map(|b| {
                serde_json::json!({
                    "id": b.id,
                    "type": format!("{:?}", b.block_type),
                    "role": format!("{:?}", b.semantic_role),
                    "text": b.text,
                    "is_boilerplate": b.is_boilerplate,
                    "links": b.links.iter().map(|l| {
                        serde_json::json!({"text": l.text, "href": l.href, "relevance": l.relevance})
                    }).collect::<Vec<_>>(),
                    "images": b.images.iter().map(|i| {
                        serde_json::json!({"src": i.src, "alt": i.alt, "is_content": i.is_content})
                    }).collect::<Vec<_>>(),
                    "version": b.version,
                })
            }).collect::<Vec<_>>(),
            "total_text_length": self.total_text_length,
            "content_blocks": self.content_block_count,
        })
    }
}
