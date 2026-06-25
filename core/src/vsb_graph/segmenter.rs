//! Page segmentation — DOM + CSS + density → VSB-Graph blocks.
//!
//! Uses VIPS-inspired segmentation to produce stable, versioned blocks.
//! Combines:
//! 1. DOM tree structure (parent-child relationships)
//! 2. CSS layout cues (class names, semantic elements)
//! 3. Text density signals (content vs navigation)

use std::collections::{HashMap, VecDeque};

use scraper::{ElementRef, Html, Node, Selector};
use sha2::{Digest, Sha256};

use crate::vsb_graph::types::*;

/// Maximum recursion depth for VSB block analysis
const MAX_SEGMENT_DEPTH: usize = 15;
/// Maximum children per block to prevent explosion
const MAX_CHILDREN_PER_BLOCK: usize = 50;
/// Minimum text length to create a block
const MIN_BLOCK_TEXT: usize = 20;

/// Segment an HTML document into a VSB-Graph.
pub fn segment_page(html: &str, url: &str) -> VSBGraph {
    let document = Html::parse_document(html);
    let mut blocks: HashMap<BlockId, VSBBlock> = HashMap::new();
    let mut roots: Vec<BlockId> = Vec::new();

    // Phase 1: Identify top-level content regions
    let top_elements = find_top_level_regions(&document);

    let mut order = 0;
    for element in &top_elements {
        // Analyze and collect all blocks recursively
        let (block, child_blocks) = analyze_element_recursive(&document, element, url, order, None);
        let id = block.id.clone();

        // Insert all child blocks
        for cb in child_blocks {
            let cb_id = cb.id.clone();
            blocks.insert(cb_id, cb);
        }

        if block.is_boilerplate {
            blocks.insert(id, block);
        } else {
            roots.push(id.clone());
            blocks.insert(id, block);
        }
        order += 1;
    }

    // Calculate stats
    let total_text_length: usize = blocks.values().map(|b| b.text.len()).sum();
    let content_blocks = blocks.values().filter(|b| !b.is_boilerplate).count();
    let boilerplate_blocks = blocks.values().filter(|b| b.is_boilerplate).count();

    // Page-level metadata
    let page_title = extract_page_title(&document);
    let page_language = extract_page_language(&document);
    let page_hash = compute_page_hash(html);

    VSBGraph {
        blocks,
        roots,
        page_url: url.to_string(),
        page_title,
        page_language,
        page_hash,
        created_at: chrono::Utc::now(),
        total_text_length,
        content_block_count: content_blocks,
        boilerplate_block_count: boilerplate_blocks,
    }
}

/// Find top-level content regions in the document.
/// Walks the DOM tree looking for semantically meaningful containers.
fn find_top_level_regions(document: &Html) -> Vec<ElementRef<'_>> {
    let mut regions = Vec::new();

    // Priority 1: Semantic HTML5 elements
    let semantic_selectors = ["main", "article", "section[role='main']"];
    for sel_str in &semantic_selectors {
        let sel = Selector::parse(sel_str);
        if let Ok(sel) = sel {
            for elem in document.select(&sel) {
                if is_content_container(&elem) {
                    regions.push(elem);
                }
            }
        }
    }

    // Priority 2: Common content containers by class
    if regions.is_empty() {
        let class_patterns = [
            "content",
            "article",
            "post",
            "main-content",
            "entry",
            "body",
        ];
        for pattern in &class_patterns {
            let sel_str = format!(".{}", pattern);
            let sel = Selector::parse(&sel_str);
            if let Ok(sel) = sel {
                for elem in document.select(&sel) {
                    if is_content_container(&elem) && elem.text().collect::<String>().len() > 100 {
                        regions.push(elem);
                        break;
                    }
                }
            }
        }
    }

    // Priority 3: Body-level direct children with substantial content
    if regions.is_empty() {
        let body_sel = Selector::parse("body");
        if let Ok(body_sel) = body_sel {
            if let Some(body) = document.select(&body_sel).next() {
                for child in body.children() {
                    if let Some(elem) = ElementRef::wrap(child) {
                        let text_len = elem.text().collect::<String>().len();
                        if text_len > 200 {
                            regions.push(elem);
                        }
                    }
                }
            }
        }
    }

    // Fallback: entire body
    if regions.is_empty() {
        let body_sel = Selector::parse("body");
        if let Ok(body_sel) = body_sel {
            if let Some(body) = document.select(&body_sel).next() {
                regions.push(body);
            }
        }
    }

    regions
}

/// Check if an element is a content container (not just a wrapper).
fn is_content_container(element: &ElementRef) -> bool {
    let text = element.text().collect::<String>();
    let text_len = text.trim().len();

    // Must have meaningful text content
    text_len > 50
}

/// Analyze a DOM element and create a VSB-Block using iterative BFS.
/// This eliminates stack overflow on deeply nested HTML.
///
/// Returns the root block plus all child blocks for insertion into the graph.
fn analyze_element_recursive(
    _document: &Html,
    element: &ElementRef,
    url: &str,
    order: usize,
    parent_id: Option<&str>,
) -> (VSBBlock, Vec<VSBBlock>) {
    // Build the root block
    let text = element.text().collect::<String>().trim().to_string();
    let html_fragment = Some(element.html());
    let (block_type, semantic_role) = classify_element_type(element);
    let links = extract_links_from_element(element, url);
    let images = extract_images_from_element(element);
    let selectors = generate_css_selectors(element);
    let content_hash = compute_content_hash(&text);
    let css_path = build_css_path(element);
    let xpath = build_xpath(element);
    let (is_boilerplate, boilerplate_score) = quick_boilerplate_check(element, &text);
    let id = format!("block-{}-{}", order, &content_hash[..8]);

    let mut block = VSBBlock {
        id: id.clone(),
        content_hash,
        block_type,
        semantic_role,
        text,
        html_fragment,
        source_selectors: selectors,
        position: Some(BlockPosition {
            order,
            depth: compute_dom_depth(element),
            is_central: is_central_content(element),
        }),
        links,
        images,
        provenance: Provenance {
            source_url: url.to_string(),
            extracted_at: chrono::Utc::now(),
            engine: "vsb_segmenter".to_string(),
            fetch_ms: 0,
            processing_ms: 0,
            css_path,
            xpath,
        },
        version: 1,
        is_boilerplate,
        boilerplate_score,
        children: Vec::new(),
        parent: parent_id.map(String::from),
        metadata: HashMap::new(),
    };

    // BFS queue: (element, child_order, depth)
    let mut queue: VecDeque<(ElementRef, usize, usize)> = VecDeque::new();
    let mut all_child_blocks: Vec<VSBBlock> = Vec::new();
    let mut child_order = 0;

    // Seed queue with direct children
    for child in element.children() {
        if let Some(child_elem) = ElementRef::wrap(child) {
            queue.push_back((child_elem, child_order, 1));
            child_order += 1;
        }
    }

    // Process BFS
    while let Some((current_elem, current_order, depth)) = queue.pop_front() {
        // Depth limit — prevent stack overflow
        if depth > MAX_SEGMENT_DEPTH {
            // Absorb remaining text into parent block instead of creating new blocks
            let remaining_text = current_elem.text().collect::<String>().trim().to_string();
            if !remaining_text.is_empty() && block.text.len() < 100_000 {
                block.text.push(' ');
                block.text.push_str(&remaining_text);
            }
            continue;
        }

        // Child count limit per block level
        if current_order >= MAX_CHILDREN_PER_BLOCK {
            continue;
        }

        let child_text = current_elem.text().collect::<String>().trim().to_string();
        if child_text.len() < MIN_BLOCK_TEXT {
            continue;
        }

        // Build child block
        let (child_block_type, child_semantic_role) = classify_element_type(&current_elem);
        let child_links = extract_links_from_element(&current_elem, url);
        let child_images = extract_images_from_element(&current_elem);
        let child_selectors = generate_css_selectors(&current_elem);
        let child_content_hash = compute_content_hash(&child_text);
        let child_css_path = build_css_path(&current_elem);
        let child_xpath = build_xpath(&current_elem);
        let (child_is_boilerplate, child_boilerplate_score) =
            quick_boilerplate_check(&current_elem, &child_text);
        let child_id = format!("block-{}-{}", current_order, &child_content_hash[..8]);

        let child_block = VSBBlock {
            id: child_id.clone(),
            content_hash: child_content_hash,
            block_type: child_block_type,
            semantic_role: child_semantic_role,
            text: child_text,
            html_fragment: Some(current_elem.html()),
            source_selectors: child_selectors,
            position: Some(BlockPosition {
                order: current_order,
                depth: compute_dom_depth(&current_elem),
                is_central: is_central_content(&current_elem),
            }),
            links: child_links,
            images: child_images,
            provenance: Provenance {
                source_url: url.to_string(),
                extracted_at: chrono::Utc::now(),
                engine: "vsb_segmenter".to_string(),
                fetch_ms: 0,
                processing_ms: 0,
                css_path: child_css_path,
                xpath: child_xpath,
            },
            version: 1,
            is_boilerplate: child_is_boilerplate,
            boilerplate_score: child_boilerplate_score,
            children: Vec::new(),
            parent: Some(id.clone()),
            metadata: HashMap::new(),
        };

        // Register child
        block.children.push(child_id.clone());
        all_child_blocks.push(child_block);

        // Enqueue grandchildren
        let mut grandchild_order = 0;
        for grandchild in current_elem.children() {
            if let Some(gc_elem) = ElementRef::wrap(grandchild) {
                if grandchild_order < MAX_CHILDREN_PER_BLOCK {
                    queue.push_back((gc_elem, grandchild_order, depth + 1));
                    grandchild_order += 1;
                }
            }
        }
    }

    (block, all_child_blocks)
}

/// Classify an element's type and semantic role.
///
/// Uses a multi-signal approach: HTML tag, CSS classes/IDs, text content,
/// schema.org microdata, and DOM context to assign a BlockType + SemanticRole.
fn classify_element_type(element: &ElementRef) -> (BlockType, SemanticRole) {
    let tag = element.value().name();
    let classes = element.value().attr("class").unwrap_or("").to_lowercase();
    let id_attr = element.value().attr("id").unwrap_or("").to_lowercase();
    let role_attr = element.value().attr("role").unwrap_or("").to_lowercase();
    let text = element.text().collect::<String>();
    let text_lower = text.to_lowercase();

    // Check for schema.org / JSON-LD type hints
    let itemtype = element.value().attr("itemtype").unwrap_or("");
    let has_schema = |keyword: &str| itemtype.contains(keyword) || text_lower.contains(keyword);

    // ── Breadcrumbs ──
    if classes.contains("breadcrumb")
        || id_attr.contains("breadcrumb")
        || role_attr.contains("breadcrumb")
        || has_schema("BreadcrumbList")
    {
        return (BlockType::Breadcrumb, SemanticRole::Navigation);
    }

    // ── Table of Contents ──
    if classes.contains("toc")
        || classes.contains("table-of-contents")
        || id_attr.contains("toc")
        || id_attr.contains("table-of-contents")
    {
        return (BlockType::TableOfContents, SemanticRole::Navigation);
    }

    // ── Navigation ──
    if tag == "nav"
        || role_attr == "navigation"
        || classes.contains("nav")
        || classes.contains("navigation")
        || classes.contains("menu")
        || id_attr.contains("nav")
        || (classes.contains("header") && classes.contains("nav"))
    {
        return (BlockType::Navigation, SemanticRole::Navigation);
    }

    // ── Pagination ──
    if classes.contains("pagination")
        || id_attr.contains("pagination")
        || role_attr.contains("pagination")
        || (text_lower.contains("page")
            && text_lower.contains("next")
            && text_lower.contains("prev"))
    {
        return (BlockType::Pagination, SemanticRole::Navigation);
    }

    // ── Header ──
    if tag == "header"
        || role_attr == "banner"
        || classes.contains("header")
        || classes.contains("banner")
        || classes.contains("topbar")
        || classes.contains("navbar")
        || id_attr.contains("header")
    {
        return (BlockType::Header, SemanticRole::Structural);
    }

    // ── Hero section ──
    if classes.contains("hero")
        || classes.contains("hero-banner")
        || classes.contains("hero-section")
        || classes.contains("jumbotron")
        || id_attr.contains("hero")
    {
        return (BlockType::Hero, SemanticRole::PrimaryContent);
    }

    // ── Footer ──
    if tag == "footer"
        || role_attr == "contentinfo"
        || classes.contains("footer")
        || classes.contains("bottombar")
        || id_attr.contains("footer")
        || (text_lower.contains("privacy policy") && text_lower.contains("terms"))
    {
        return (BlockType::Footer, SemanticRole::Structural);
    }

    // ── Sidebar/Aside ──
    if tag == "aside"
        || role_attr == "complementary"
        || classes.contains("sidebar")
        || classes.contains("aside")
        || classes.contains("widget-area")
        || id_attr.contains("sidebar")
    {
        return (BlockType::Sidebar, SemanticRole::SupportingContent);
    }

    // ── Search ──
    if classes.contains("search")
        || id_attr.contains("search")
        || role_attr == "search"
        || has_type_search_input(element)
    {
        return (BlockType::Search, SemanticRole::Interactive);
    }

    // ── Login / Auth ──
    if classes.contains("login")
        || classes.contains("signin")
        || classes.contains("sign-in")
        || classes.contains("auth")
        || id_attr.contains("login")
        || id_attr.contains("signin")
        || (text_lower.contains("sign in") && text_lower.contains("password"))
        || (text_lower.contains("log in") && text_lower.contains("password"))
    {
        return (BlockType::Login, SemanticRole::Interactive);
    }

    // ── Form ──
    if tag == "form"
        || role_attr == "form"
        || classes.contains("form")
        || classes.contains("contact-form")
        || classes.contains("signup-form")
        || id_attr.contains("form")
    {
        return (BlockType::Form, SemanticRole::Interactive);
    }

    // ── Filter controls ──
    if classes.contains("filter")
        || classes.contains("facet")
        || id_attr.contains("filter")
        || id_attr.contains("facet")
        || text_lower.contains("filter by")
        || text_lower.contains("sort by")
    {
        return (BlockType::Filter, SemanticRole::Interactive);
    }

    // ── Call-to-Action ──
    if classes.contains("cta")
        || classes.contains("call-to-action")
        || classes.contains("signup-prompt")
        || id_attr.contains("cta")
    {
        return (BlockType::CallToAction, SemanticRole::Commercial);
    }

    // ── Feature grid ──
    if classes.contains("feature")
        || classes.contains("benefit")
        || id_attr.contains("features")
        || id_attr.contains("benefits")
    {
        return (BlockType::FeatureGrid, SemanticRole::PrimaryContent);
    }

    // ── Testimonial ──
    if classes.contains("testimonial")
        || classes.contains("customer-quote")
        || id_attr.contains("testimonial")
        || has_schema("Review") && !text_lower.contains("comment")
    {
        return (BlockType::Testimonial, SemanticRole::SupportingContent);
    }

    // ── Pricing ──
    if classes.contains("pricing")
        || classes.contains("price-table")
        || id_attr.contains("pricing")
        || (classes.contains("price") && text_lower.contains("$"))
    {
        return (BlockType::Pricing, SemanticRole::Commercial);
    }

    // ── Shopping Cart ──
    if classes.contains("cart")
        || classes.contains("basket")
        || classes.contains("shopping-cart")
        || id_attr.contains("cart")
        || id_attr.contains("basket")
        || has_schema("Order")
    {
        return (BlockType::Cart, SemanticRole::Interactive);
    }

    // ── Product detail page ──
    if classes.contains("product-detail")
        || classes.contains("product-page")
        || classes.contains("pdp")
        || id_attr.contains("product-detail")
        || has_schema("Product")
    {
        return (BlockType::ProductDetail, SemanticRole::PrimaryContent);
    }

    // ── Product listing page ──
    if classes.contains("product-list")
        || classes.contains("product-grid")
        || classes.contains("category-page")
        || classes.contains("plp")
        || id_attr.contains("product-list")
    {
        return (BlockType::ProductListing, SemanticRole::PrimaryContent);
    }

    // ── Product card ──
    if classes.contains("product-card")
        || classes.contains("product-item")
        || (classes.contains("card") && text_lower.contains("add to cart"))
    {
        return (BlockType::ProductCard, SemanticRole::Commercial);
    }

    // ── Reviews / Ratings ──
    if classes.contains("review")
        || classes.contains("rating")
        || classes.contains("ratings-summary")
        || id_attr.contains("review")
        || has_schema("AggregateRating")
    {
        return (BlockType::Review, SemanticRole::UserGenerated);
    }

    // ── Comments ──
    if classes.contains("comment")
        || classes.contains("comments")
        || id_attr.contains("comment")
        || has_schema("Comment")
    {
        return (BlockType::Comment, SemanticRole::UserGenerated);
    }

    // ── User profile ──
    if classes.contains("profile")
        || classes.contains("user-card")
        || classes.contains("author-box")
        || classes.contains("author-info")
        || id_attr.contains("profile")
        || has_schema("Person")
    {
        return (BlockType::UserProfile, SemanticRole::SupportingContent);
    }

    // ── Social feed ──
    if classes.contains("feed")
        || classes.contains("timeline")
        || classes.contains("stream")
        || id_attr.contains("feed")
        || id_attr.contains("timeline")
    {
        return (BlockType::Feed, SemanticRole::UserGenerated);
    }

    // ── Forum thread ──
    if classes.contains("forum")
        || classes.contains("thread")
        || classes.contains("topic")
        || id_attr.contains("forum")
        || id_attr.contains("thread")
    {
        return (BlockType::Forum, SemanticRole::UserGenerated);
    }

    // ── FAQ ──
    if classes.contains("faq")
        || classes.contains("q-and-a")
        || id_attr.contains("faq")
        || has_schema("FAQPage")
        || text_lower.contains("frequently asked questions")
    {
        return (BlockType::FAQ, SemanticRole::PrimaryContent);
    }

    // ── Changelog ──
    if classes.contains("changelog")
        || classes.contains("release-notes")
        || id_attr.contains("changelog")
        || id_attr.contains("release-notes")
    {
        return (BlockType::Changelog, SemanticRole::PrimaryContent);
    }

    // ── Tutorial / How-to ──
    if classes.contains("tutorial")
        || classes.contains("how-to")
        || classes.contains("howto")
        || id_attr.contains("tutorial")
        || has_schema("HowTo")
        || text_lower.contains("step 1")
    {
        return (BlockType::Tutorial, SemanticRole::PrimaryContent);
    }

    // ── Documentation ──
    if classes.contains("docs")
        || classes.contains("documentation")
        || id_attr.contains("docs")
        || id_attr.contains("documentation")
    {
        return (BlockType::Documentation, SemanticRole::PrimaryContent);
    }

    // ── Specification ──
    if classes.contains("spec")
        || classes.contains("specification")
        || classes.contains("standard")
        || id_attr.contains("spec")
    {
        return (BlockType::Specification, SemanticRole::PrimaryContent);
    }

    // ── Blog post ──
    if classes.contains("blog-post")
        || classes.contains("single-post")
        || classes.contains("blog-entry")
        || id_attr.contains("blog-post")
        || (has_schema("BlogPosting") && !classes.contains("product"))
    {
        return (BlockType::BlogPost, SemanticRole::PrimaryContent);
    }

    // ── Article/Main Content ──
    if tag == "article"
        || tag == "main"
        || role_attr == "main"
        || classes.contains("content")
        || classes.contains("article")
        || classes.contains("post")
        || classes.contains("entry-content")
        || classes.contains("mw-parser-output")  // Wikipedia
        || id_attr.contains("content")
        || id_attr.contains("bodyContent")
    // Wikipedia
    {
        return (BlockType::Article, SemanticRole::PrimaryContent);
    }

    // ── Section (check context) ──
    if tag == "section" {
        if classes.contains("content") || classes.contains("article") {
            return (BlockType::Article, SemanticRole::PrimaryContent);
        }
        let has_heading = text_lower.contains("edit") || text_lower.contains("reference");
        if has_heading {
            return (BlockType::Article, SemanticRole::PrimaryContent);
        }
        return (BlockType::Generic, SemanticRole::Unknown);
    }

    // ── API Documentation ──
    if classes.contains("api-doc")
        || classes.contains("endpoint")
        || id_attr.contains("api-doc")
        || (text_lower.contains("get /")
            || text_lower.contains("post /")
            || text_lower.contains("put /"))
    {
        return (BlockType::APIDocumentation, SemanticRole::PrimaryContent);
    }

    // ── Chart / Visualization ──
    if tag == "canvas"
        || classes.contains("chart")
        || classes.contains("graph")
        || classes.contains("visualization")
        || id_attr.contains("chart")
    {
        return (BlockType::Chart, SemanticRole::PrimaryContent);
    }

    // ── Code blocks ──
    if tag == "pre" || tag == "code" {
        return (BlockType::Code, SemanticRole::PrimaryContent);
    }

    // ── Tables ──
    if tag == "table" {
        return (BlockType::Table, SemanticRole::PrimaryContent);
    }

    // ── Media embeds ──
    if tag == "video" || tag == "audio" {
        return (BlockType::Media, SemanticRole::PrimaryContent);
    }
    if tag == "iframe" {
        let src = element.value().attr("src").unwrap_or("");
        if src.contains("youtube")
            || src.contains("vimeo")
            || src.contains("spotify")
            || src.contains("soundcloud")
            || src.contains("wistia")
        {
            return (BlockType::Media, SemanticRole::PrimaryContent);
        }
    }
    if classes.contains("video-embed")
        || classes.contains("media-player")
        || id_attr.contains("video")
        || id_attr.contains("media")
    {
        return (BlockType::Media, SemanticRole::PrimaryContent);
    }

    // ── Ads ──
    if classes.contains("ad")
        || classes.contains("advert")
        || classes.contains("sponsor")
        || classes.contains("promoted")
        || id_attr.contains("ad")
        || text_lower.contains("advertisement")
        || text_lower.contains("sponsored by")
    {
        return (BlockType::Advertisement, SemanticRole::Commercial);
    }

    // ── Headings within content ──
    if tag.starts_with('h') && tag.len() == 2 {
        return (BlockType::Generic, SemanticRole::PrimaryContent);
    }

    // ── Paragraphs within content ──
    if tag == "p" {
        return (BlockType::Generic, SemanticRole::PrimaryContent);
    }

    // ── Div/span inside article context ──
    if tag == "div" || tag == "span" {
        if let Some(parent_node) = element.parent() {
            if let Some(parent_elem) = ElementRef::wrap(parent_node) {
                let parent_tag = parent_elem.value().name();
                let parent_classes = parent_elem
                    .value()
                    .attr("class")
                    .unwrap_or("")
                    .to_lowercase();
                if parent_tag == "article"
                    || parent_tag == "main"
                    || parent_classes.contains("content")
                    || parent_classes.contains("article")
                    || parent_classes.contains("mw-parser-output")
                {
                    return (BlockType::Generic, SemanticRole::PrimaryContent);
                }
            }
        }
        return (BlockType::Generic, SemanticRole::Unknown);
    }

    // ── Generic fallback with ancestor role inference ──
    let (block_type, semantic_role) = (BlockType::Generic, SemanticRole::Unknown);

    let inferred_role = infer_ancestor_role(element);
    if inferred_role != SemanticRole::Unknown {
        return (block_type, inferred_role);
    }

    (block_type, semantic_role)
}

/// Check if an element or its descendants contains a search input.
fn has_type_search_input(element: &ElementRef) -> bool {
    if let Ok(sel) = scraper::Selector::parse("input[type='search']") {
        return element.select(&sel).next().is_some();
    }
    false
}

/// Walk up the DOM tree to infer semantic role from the nearest meaningful ancestor.
fn infer_ancestor_role(element: &ElementRef) -> SemanticRole {
    let mut current: scraper::ElementRef = *element;

    for _depth in 0..10 {
        if let Some(parent_node) = current.parent() {
            if let Some(parent_elem) = ElementRef::wrap(parent_node) {
                let (parent_type, parent_role) = classify_element_type(&parent_elem);

                // If parent has a meaningful role, inherit it
                if parent_role != SemanticRole::Unknown {
                    return parent_role;
                }

                // If parent is a known structural type, use its role
                match parent_type {
                    BlockType::Article
                    | BlockType::BlogPost
                    | BlockType::Documentation
                    | BlockType::Tutorial
                    | BlockType::Specification
                    | BlockType::Changelog
                    | BlockType::FAQ
                    | BlockType::Hero
                    | BlockType::FeatureGrid => return SemanticRole::PrimaryContent,
                    BlockType::Navigation
                    | BlockType::Breadcrumb
                    | BlockType::Pagination
                    | BlockType::TableOfContents => return SemanticRole::Navigation,
                    BlockType::Header | BlockType::Footer => return SemanticRole::Structural,
                    BlockType::Sidebar | BlockType::Testimonial | BlockType::UserProfile => {
                        return SemanticRole::SupportingContent
                    }
                    BlockType::Advertisement | BlockType::Pricing | BlockType::ProductCard => {
                        return SemanticRole::Commercial
                    }
                    BlockType::Comment | BlockType::Review | BlockType::Feed | BlockType::Forum => {
                        return SemanticRole::UserGenerated
                    }
                    BlockType::Search
                    | BlockType::Form
                    | BlockType::Login
                    | BlockType::Filter
                    | BlockType::Cart => return SemanticRole::Interactive,
                    BlockType::ProductDetail
                    | BlockType::ProductListing
                    | BlockType::Table
                    | BlockType::Code
                    | BlockType::Chart
                    | BlockType::APIDocumentation
                    | BlockType::Media
                    | BlockType::CallToAction => return SemanticRole::PrimaryContent,
                    _ => {}
                }

                // Check parent class names for context
                let parent_classes = parent_elem
                    .value()
                    .attr("class")
                    .unwrap_or("")
                    .to_lowercase();
                let parent_id = parent_elem.value().attr("id").unwrap_or("").to_lowercase();

                if parent_classes.contains("content")
                    || parent_classes.contains("article")
                    || parent_classes.contains("mw-parser-output")
                    || parent_id.contains("content")
                    || parent_id.contains("bodyContent")
                {
                    return SemanticRole::PrimaryContent;
                }

                if parent_classes.contains("nav") || parent_classes.contains("menu") {
                    return SemanticRole::Navigation;
                }

                if parent_classes.contains("sidebar") || parent_classes.contains("widget") {
                    return SemanticRole::SupportingContent;
                }

                if parent_classes.contains("ad")
                    || parent_classes.contains("advert")
                    || parent_classes.contains("pricing")
                {
                    return SemanticRole::Commercial;
                }

                if parent_classes.contains("comment")
                    || parent_classes.contains("review")
                    || parent_classes.contains("forum")
                {
                    return SemanticRole::UserGenerated;
                }

                current = parent_elem;
            } else {
                break;
            }
        } else {
            break;
        }
    }

    SemanticRole::Unknown
}

/// Quick boilerplate check using six-dimensional classifier (simplified).
fn quick_boilerplate_check(element: &ElementRef, text: &str) -> (bool, f64) {
    let text_len = text.len();
    if text_len == 0 {
        return (true, 1.0);
    }

    let tag = element.value().name();
    let classes = element.value().attr("class").unwrap_or("").to_lowercase();

    // Dimension 1: Length — boilerplate tends to be short and uniform
    let length_score = if text_len < 50 { 0.8 } else { 0.2 };

    // Dimension 2: Density — boilerplate has low text-to-HTML ratio
    let html_len = element.html().len();
    let density = if html_len > 0 {
        text_len as f64 / html_len as f64
    } else {
        0.0
    };
    let density_score = if density < 0.1 { 0.8 } else { 0.2 };

    // Dimension 3: Link ratio — boilerplate is link-heavy
    let link_count = element
        .descendants()
        .filter(|n| {
            if let Node::Element(el) = n.value() {
                el.name() == "a"
            } else {
                false
            }
        })
        .count();
    let word_count = text.split_whitespace().count();
    let link_ratio = if word_count > 0 {
        link_count as f64 / word_count as f64
    } else {
        0.0
    };
    let link_score = if link_ratio > 0.5 { 0.8 } else { 0.2 };

    // Dimension 4: Structure — boilerplate uses repetitive patterns
    let structure_score = if tag == "nav" || tag == "footer" || tag == "header" {
        0.9
    } else if classes.contains("nav") || classes.contains("footer") || classes.contains("header") {
        0.8
    } else {
        0.1
    };

    // Dimension 5: Uniqueness — boilerplate appears on many pages
    let uniqueness_score = 0.5; // Can't check without cross-page analysis

    // Dimension 6: Freshness — boilerplate rarely changes
    let freshness_score = 0.5; // Can't check without historical data

    // Combined score (weighted)
    let score = length_score * 0.15
        + density_score * 0.20
        + link_score * 0.20
        + structure_score * 0.25
        + uniqueness_score * 0.10
        + freshness_score * 0.10;

    (score > 0.6, score)
}

/// Check if an element is structural (worthy of becoming a sub-block).
fn is_structural_element(element: &ElementRef) -> bool {
    let tag = element.value().name();
    matches!(
        tag,
        "div"
            | "section"
            | "article"
            | "main"
            | "header"
            | "footer"
            | "nav"
            | "aside"
            | "ul"
            | "ol"
            | "table"
            | "form"
    )
}

/// Generate CSS selectors for an element.
fn generate_css_selectors(element: &ElementRef) -> Vec<String> {
    let mut selectors = Vec::new();

    // ID selector
    if let Some(id) = element.value().attr("id") {
        selectors.push(format!("#{}", id));
    }

    // Class selector
    if let Some(classes) = element.value().attr("class") {
        let class_selector = classes
            .split_whitespace()
            .map(|c| format!(".{}", c))
            .collect::<Vec<_>>()
            .join("");
        if !class_selector.is_empty() {
            selectors.push(format!("{}{}", element.value().name(), class_selector));
        }
    }

    // Tag selector
    selectors.push(element.value().name().to_string());

    selectors
}

/// Build a full CSS path to an element.
fn build_css_path(element: &ElementRef) -> String {
    let mut path = Vec::new();
    let mut current: scraper::ElementRef = *element;

    loop {
        let tag = current.value().name();
        let segment = if let Some(id) = current.value().attr("id") {
            format!("#{}", id)
        } else if let Some(classes) = current.value().attr("class") {
            let first_class = classes.split_whitespace().next().unwrap_or("");
            format!("{}.{}", tag, first_class)
        } else {
            tag.to_string()
        };

        path.push(segment);

        if let Some(parent) = current.parent() {
            if let Some(parent_elem) = ElementRef::wrap(parent) {
                current = parent_elem;
                continue;
            }
        }
        break;
    }

    path.reverse();
    path.join(" > ")
}

/// Build an XPath to an element.
fn build_xpath(element: &ElementRef) -> String {
    // Simplified XPath — just the tag chain
    let mut path = Vec::new();
    let mut current: scraper::ElementRef = *element;

    loop {
        path.push(current.value().name().to_string());

        if let Some(parent) = current.parent() {
            if let Some(parent_elem) = ElementRef::wrap(parent) {
                current = parent_elem;
                continue;
            }
        }
        break;
    }

    path.reverse();
    format!("/{}", path.join("/"))
}

/// Compute DOM depth of an element.
fn compute_dom_depth(element: &ElementRef) -> usize {
    let mut depth = 0;
    let mut current: scraper::ElementRef = *element;

    loop {
        if let Some(parent) = current.parent() {
            if let Some(parent_elem) = ElementRef::wrap(parent) {
                current = parent_elem;
                depth += 1;
                continue;
            }
        }
        break;
    }

    depth
}

/// Check if an element is in the central content area.
fn is_central_content(element: &ElementRef) -> bool {
    let tag = element.value().name();
    let classes = element.value().attr("class").unwrap_or("").to_lowercase();
    let id = element.value().attr("id").unwrap_or("").to_lowercase();

    tag == "article"
        || tag == "main"
        || classes.contains("content")
        || classes.contains("article")
        || classes.contains("post")
        || classes.contains("entry")
        || id.contains("content")
        || id.contains("main")
}

/// Extract page title.
fn extract_page_title(document: &Html) -> Option<String> {
    // Try <title> first
    if let Ok(sel) = Selector::parse("title") {
        if let Some(elem) = document.select(&sel).next() {
            let title = elem.text().collect::<String>().trim().to_string();
            if !title.is_empty() {
                return Some(title);
            }
        }
    }

    // Fall back to <h1>
    if let Ok(sel) = Selector::parse("h1") {
        if let Some(elem) = document.select(&sel).next() {
            let title = elem.text().collect::<String>().trim().to_string();
            if !title.is_empty() {
                return Some(title);
            }
        }
    }

    None
}

/// Extract page language.
fn extract_page_language(document: &Html) -> Option<String> {
    if let Ok(sel) = Selector::parse("html") {
        if let Some(elem) = document.select(&sel).next() {
            if let Some(lang) = elem.value().attr("lang") {
                return Some(lang.to_string());
            }
        }
    }
    None
}

/// Compute a hash of the page content for versioning.
fn compute_page_hash(html: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(html.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Compute a content fingerprint for a block.
fn compute_content_hash(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.trim().as_bytes());
    let result = hasher.finalize();
    format!("{:x}", result)
}

/// Extract links from an element.
fn extract_links_from_element(element: &ElementRef, base_url: &str) -> Vec<BlockLink> {
    let base_domain = url::Url::parse(base_url)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_string()));

    if let Ok(sel) = Selector::parse("a[href]") {
        element
            .select(&sel)
            .filter_map(|a| {
                let href = a.value().attr("href")?;
                let text = a.text().collect::<String>().trim().to_string();

                if text.is_empty() || href.starts_with("javascript:") || href.starts_with('#') {
                    return None;
                }

                let resolved = if href.starts_with("http") {
                    href.to_string()
                } else if let Ok(base) = url::Url::parse(base_url) {
                    base.join(href)
                        .map(|u| u.to_string())
                        .unwrap_or(href.to_string())
                } else {
                    href.to_string()
                };

                let is_internal = if let Ok(parsed) = url::Url::parse(&resolved) {
                    parsed.host_str() == base_domain.as_deref()
                } else {
                    true
                };

                Some(BlockLink {
                    text,
                    href: resolved,
                    is_internal,
                    relevance: if is_internal { 0.7 } else { 0.5 },
                })
            })
            .collect()
    } else {
        Vec::new()
    }
}

/// Extract images from an element.
fn extract_images_from_element(element: &ElementRef) -> Vec<BlockImage> {
    if let Ok(sel) = Selector::parse("img[src]") {
        element
            .select(&sel)
            .filter_map(|img| {
                let src = img.value().attr("src")?.to_string();
                let alt = img.value().attr("alt").map(String::from);

                // Skip tracking pixels and tiny images
                let width = img
                    .value()
                    .attr("width")
                    .and_then(|w| w.parse::<u32>().ok());
                let height = img
                    .value()
                    .attr("height")
                    .and_then(|h| h.parse::<u32>().ok());

                let is_content = alt.is_some()
                    || width.map(|w| w > 100).unwrap_or(true)
                    || height.map(|h| h > 100).unwrap_or(true);

                Some(BlockImage {
                    src,
                    alt,
                    width,
                    height,
                    is_content,
                })
            })
            .collect()
    } else {
        Vec::new()
    }
}
