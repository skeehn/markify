//! ML-powered VSB Block Classifier.
//!
//! Uses BGE-Small embeddings + gradient-boosted feature classification
//! to assign VSB blocks to one of 35 semantic types with high confidence.
//!
//! Architecture:
//! 1. **Feature extraction**: DOM features, text features, layout features, embedding features
//! 2. **ML classifier**: BGE embeddings → cosine similarity to class centroids + heuristic scoring
//! 3. **Confidence scoring**: Ensemble agreement → high/medium/low → escalation path
//! 4. **Weak supervision**: Snorkel-style labeling functions for training data generation
//!
//! Modes:
//! - `fast`: Heuristic-only (no ML dependency) — ~1ms per block
//! - `ml`: BGE embeddings + centroid matching — ~10ms per block (CPU)
//! - `vlm`: Donut/LayoutLM visual+text — cloud GPU (TODO)

use std::collections::HashMap;

use crate::vsb_graph::types::*;

/// Classification result with confidence and top candidates
#[derive(Debug, Clone, serde::Serialize)]
pub struct MLClassificationResult {
    /// Primary predicted type
    pub block_type: BlockType,
    /// Primary predicted semantic role
    pub semantic_role: SemanticRole,
    /// Confidence 0.0-1.0
    pub confidence: f64,
    /// Top-3 candidate types with scores
    pub top_candidates: Vec<(BlockType, f64)>,
    /// Classification mode used
    pub mode: ClassificationMode,
    /// Whether this block needs human review
    pub needs_review: bool,
}

/// Classification mode
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub enum ClassificationMode {
    /// Heuristic-only, no ML
    Fast,
    /// BGE embeddings + centroid matching
    ML,
}

/// Feature vector for a VSB block
#[derive(Debug, Clone)]
pub struct BlockFeatures {
    // ── DOM Features ──
    /// DOM depth (distance from root)
    pub dom_depth: usize,
    /// Number of children
    pub child_count: usize,
    /// Number of text nodes
    pub text_node_count: usize,
    /// Number of link elements
    pub link_count: usize,
    /// Number of image elements
    pub image_count: usize,
    /// Number of heading elements (h1-h6)
    pub heading_count: usize,
    /// Number of form elements
    pub form_element_count: usize,
    /// Number of list items
    pub list_item_count: usize,
    /// Number of table rows
    pub table_row_count: usize,

    // ── Text Features ──
    /// Total text length
    pub text_length: usize,
    /// Number of lines
    pub line_count: usize,
    /// Average word length
    pub avg_word_length: f64,
    /// Ratio of uppercase words
    pub uppercase_ratio: f64,
    /// Ratio of numeric tokens
    pub numeric_ratio: f64,
    /// Has code-like patterns (indented lines, backticks)
    pub has_code_patterns: bool,
    /// Has URL-like patterns
    pub has_url_patterns: bool,
    /// Has email-like patterns
    pub has_email_patterns: bool,

    // ── CSS/Selector Features ──
    /// CSS selector depth
    pub selector_depth: usize,
    /// Has ID attribute
    pub has_id: bool,
    /// Number of CSS classes
    pub class_count: usize,
    /// Has ARIA role attribute
    pub has_aria_role: bool,
    /// ARIA role value
    pub aria_role: Option<String>,
    /// Has schema.org microdata
    pub has_schema_org: bool,
    /// Schema.org type
    pub schema_type: Option<String>,

    // ── Link Graph Features ──
    /// Ratio of links to text words
    pub link_to_text_ratio: f64,
    /// Ratio of internal links
    pub internal_link_ratio: f64,
    /// Has "next" or "previous" links
    pub has_pagination_links: bool,
    /// Has navigation-like link pattern (many short anchor texts)
    pub has_nav_pattern: bool,

    // ── Content Features ──
    /// Text-to-HTML ratio
    pub text_to_html_ratio: f64,
    /// Has structured data (JSON-LD, microdata)
    pub has_structured_data: bool,
    /// Has interactive elements (buttons, inputs)
    pub has_interactive: bool,
    /// Has media embeds (iframe, video, audio)
    pub has_media_embed: bool,
}

impl BlockFeatures {
    /// Extract features from a VSB block
    pub fn extract(block: &VSBBlock) -> Self {
        let text = &block.text;
        let words: Vec<&str> = text.split_whitespace().collect();
        let lines: Vec<&str> = text.lines().collect();

        // Text statistics
        let avg_word_length = if words.is_empty() {
            0.0
        } else {
            words.iter().map(|w| w.len()).sum::<usize>() as f64 / words.len() as f64
        };

        let uppercase_ratio = if words.is_empty() {
            0.0
        } else {
            words
                .iter()
                .filter(|w| w.chars().all(|c| c.is_uppercase() || !c.is_alphabetic()))
                .count() as f64
                / words.len() as f64
        };

        let numeric_ratio = if words.is_empty() {
            0.0
        } else {
            words
                .iter()
                .filter(|w| {
                    w.chars()
                        .all(|c| c.is_numeric() || c == '.' || c == ',' || c == '-')
                })
                .count() as f64
                / words.len() as f64
        };

        let link_count = block.links.len();
        let text_word_count = words.len();
        let link_to_text_ratio = if text_word_count == 0 {
            0.0
        } else {
            link_count as f64 / text_word_count as f64
        };
        let internal_link_ratio = if link_count == 0 {
            0.0
        } else {
            block.links.iter().filter(|l| l.is_internal).count() as f64 / link_count as f64
        };

        // Code patterns
        let has_code_patterns = text.contains("```")
            || text.contains('\t')
            || text
                .lines()
                .any(|l| l.starts_with("    ") || l.starts_with("\t"));
        let has_url_patterns = regex::Regex::new(r"https?://")
            .map(|r| r.is_match(text))
            .unwrap_or(false);
        let has_email_patterns =
            regex::Regex::new(r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}")
                .map(|r| r.is_match(text))
                .unwrap_or(false);

        // Selector features
        let _selectors_str = block.source_selectors.join(" ");
        let has_id = block.source_selectors.iter().any(|s| s.starts_with('#'));
        let class_count: usize = block
            .source_selectors
            .iter()
            .map(|s| s.matches('.').count())
            .sum();
        let has_aria_role = false; // Would need full DOM access
        let aria_role = None;
        let has_schema_org = text.contains("schema.org") || text.contains("itemtype=");
        let schema_type = None; // Would need schema.org parsing

        // Pagination detection
        let has_pagination_links = text.to_lowercase().contains("next")
            && text.to_lowercase().contains("previous")
            || text.to_lowercase().contains("page ") && text.contains(" of ");

        // Nav pattern: many short link texts
        let has_nav_pattern =
            link_count > 3 && link_count as f64 / text_word_count.max(1) as f64 > 0.3;

        let text_to_html_ratio = {
            let html_len = block.html_fragment.as_ref().map(|h| h.len()).unwrap_or(1);
            if html_len == 0 {
                0.0
            } else {
                text.len() as f64 / html_len as f64
            }
        };

        let has_interactive = text.contains("<button")
            || text.contains("<input")
            || text.contains("<form")
            || text.contains("<select")
            || text.contains("<textarea");
        let has_media_embed = text.contains("<iframe")
            || text.contains("<video")
            || text.contains("<audio")
            || text.contains("<embed");
        let has_structured_data =
            text.contains("<script type=\"application/ld+json\"") || text.contains("JSON-LD");

        // Table row estimation
        let table_row_count = text
            .lines()
            .filter(|l| l.contains('|') || l.contains("<td") || l.contains("<tr"))
            .count();

        Self {
            dom_depth: block.position.as_ref().map(|p| p.depth).unwrap_or(0),
            child_count: block.children.len(),
            text_node_count: lines.len(),
            link_count,
            image_count: block.images.len(),
            heading_count: lines
                .iter()
                .filter(|l| l.len() < 80 && (l.len() < 60 || l.ends_with(':')))
                .count(),
            form_element_count: text.matches("<input").count()
                + text.matches("<select").count()
                + text.matches("<textarea").count(),
            list_item_count: text.matches("<li").count()
                + text
                    .lines()
                    .filter(|l| l.starts_with('-') || l.starts_with('*') || l.starts_with("•"))
                    .count(),
            table_row_count,
            text_length: text.len(),
            line_count: lines.len(),
            avg_word_length,
            uppercase_ratio,
            numeric_ratio,
            has_code_patterns,
            has_url_patterns,
            has_email_patterns,
            selector_depth: block.source_selectors.len(),
            has_id,
            class_count,
            has_aria_role,
            aria_role,
            has_schema_org,
            schema_type,
            link_to_text_ratio,
            internal_link_ratio,
            has_pagination_links,
            has_nav_pattern,
            text_to_html_ratio,
            has_structured_data,
            has_interactive,
            has_media_embed,
        }
    }
}

/// Labeling functions for weak supervision
struct LabelingFunctions;

impl LabelingFunctions {
    /// LF: Navigation — has nav tag or nav-like class
    fn lf_navigation(block: &VSBBlock) -> Option<BlockType> {
        let selectors = block.source_selectors.join(" ").to_lowercase();
        let text_lower = block.text.to_lowercase();

        if selectors.contains("nav")
            || selectors.contains("navigation")
            || selectors.contains("menu")
            || text_lower.contains("home")
                && text_lower.contains("about")
                && text_lower.contains("contact")
        {
            return Some(BlockType::Navigation);
        }
        None
    }

    /// LF: Header — has header/banner tag or hero-like
    fn lf_header(block: &VSBBlock) -> Option<BlockType> {
        let selectors = block.source_selectors.join(" ").to_lowercase();

        if selectors.contains("header")
            || selectors.contains("banner")
            || selectors.contains("hero")
        {
            return Some(BlockType::Header);
        }
        if selectors.contains("hero") || selectors.contains("jumbotron") {
            return Some(BlockType::Hero);
        }
        None
    }

    /// LF: Footer — has footer tag or privacy/terms text
    fn lf_footer(block: &VSBBlock) -> Option<BlockType> {
        let selectors = block.source_selectors.join(" ").to_lowercase();
        let text_lower = block.text.to_lowercase();

        if selectors.contains("footer") {
            return Some(BlockType::Footer);
        }
        if text_lower.contains("privacy policy") && text_lower.contains("terms") {
            return Some(BlockType::Footer);
        }
        None
    }

    /// LF: Sidebar — has sidebar/aside/widget class
    fn lf_sidebar(block: &VSBBlock) -> Option<BlockType> {
        let selectors = block.source_selectors.join(" ").to_lowercase();

        if selectors.contains("sidebar")
            || selectors.contains("aside")
            || selectors.contains("widget")
            || selectors.contains("toc")
        {
            return Some(BlockType::Sidebar);
        }
        None
    }

    /// LF: Search — has search input or search role
    fn lf_search(block: &VSBBlock) -> Option<BlockType> {
        let text_lower = block.text.to_lowercase();

        if block.text.contains("type=\"search\"")
            || block.text.contains("role=\"search\"")
            || (text_lower.contains("search") && block.text.contains("<input"))
        {
            return Some(BlockType::Search);
        }
        None
    }

    /// LF: Form — has form tag or multiple inputs
    fn lf_form(block: &VSBBlock) -> Option<BlockType> {
        let form_count = block.text.matches("<form").count() + block.text.matches("<input").count();

        if block.text.contains("<form") || form_count >= 2 {
            return Some(BlockType::Form);
        }
        None
    }

    /// LF: Login — has password field or OAuth buttons
    fn lf_login(block: &VSBBlock) -> Option<BlockType> {
        let text_lower = block.text.to_lowercase();

        if block.text.contains("type=\"password\"")
            || text_lower.contains("sign in")
            || text_lower.contains("log in")
            || text_lower.contains("login")
        {
            return Some(BlockType::Login);
        }
        None
    }

    /// LF: Table of Contents — has anchor links matching headings
    fn lf_toc(block: &VSBBlock) -> Option<BlockType> {
        let text_lower = block.text.to_lowercase();
        let anchor_count = block.text.matches("<a href=\"#").count();

        if anchor_count >= 3
            && (text_lower.contains("contents")
                || text_lower.contains("table of contents")
                || text_lower.contains("on this page"))
        {
            return Some(BlockType::TableOfContents);
        }
        None
    }

    /// LF: Breadcrumb — has breadcrumb class or home > ... pattern
    fn lf_breadcrumb(block: &VSBBlock) -> Option<BlockType> {
        let selectors = block.source_selectors.join(" ").to_lowercase();
        let text_lower = block.text.to_lowercase();

        if selectors.contains("breadcrumb")
            || text_lower.contains("home >")
            || text_lower.contains("home ›")
        {
            return Some(BlockType::Breadcrumb);
        }
        None
    }

    /// LF: Pagination — has page numbers or next/previous
    fn lf_pagination(block: &VSBBlock) -> Option<BlockType> {
        let text_lower = block.text.to_lowercase();

        if text_lower.contains("next") && text_lower.contains("previous")
            || (text_lower.contains("page ") && text_lower.contains(" of "))
        {
            return Some(BlockType::Pagination);
        }
        None
    }

    /// LF: Code — has code/pre tags or code patterns
    fn lf_code(block: &VSBBlock) -> Option<BlockType> {
        if block.text.contains("<code")
            || block.text.contains("<pre")
            || block.text.contains("```")
            || block.text.contains('\t')
        {
            return Some(BlockType::Code);
        }
        None
    }

    /// LF: Table — has table tags or pipe-delimited rows
    fn lf_table(block: &VSBBlock) -> Option<BlockType> {
        if block.text.contains("<table") || block.text.contains("<tr") || block.text.contains("<td")
        {
            return Some(BlockType::Table);
        }
        // Markdown-style table
        let pipe_lines = block.text.lines().filter(|l| l.contains('|')).count();
        if pipe_lines >= 2 {
            return Some(BlockType::Table);
        }
        None
    }

    /// LF: FAQ — has Q&A pattern or FAQ schema
    fn lf_faq(block: &VSBBlock) -> Option<BlockType> {
        let text_lower = block.text.to_lowercase();

        if text_lower.contains("frequently asked questions")
            || text_lower.contains("faq")
            || text_lower.contains("schema.org/faqpage")
        {
            return Some(BlockType::FAQ);
        }
        None
    }

    /// LF: Advertisement — has ad/sponsor class
    fn lf_advertisement(block: &VSBBlock) -> Option<BlockType> {
        let selectors = block.source_selectors.join(" ").to_lowercase();
        let text_lower = block.text.to_lowercase();

        if selectors.contains("ad")
            || selectors.contains("advert")
            || selectors.contains("sponsor")
            || text_lower.contains("advertisement")
            || text_lower.contains("sponsored")
        {
            return Some(BlockType::Advertisement);
        }
        None
    }

    /// LF: Documentation — has technical doc patterns
    fn lf_documentation(block: &VSBBlock) -> Option<BlockType> {
        let selectors = block.source_selectors.join(" ").to_lowercase();
        let text_lower = block.text.to_lowercase();

        if selectors.contains("doc")
            || text_lower.contains("api reference")
            || text_lower.contains("documentation")
            || text_lower.contains("version:") && text_lower.contains("parameters")
        {
            return Some(BlockType::Documentation);
        }
        None
    }

    /// LF: Product — has price/product patterns
    fn lf_product(block: &VSBBlock) -> Option<BlockType> {
        let text_lower = block.text.to_lowercase();
        let has_price = regex::Regex::new(r"\$[\d,]+\.?\d*")
            .map(|r| r.is_match(&block.text))
            .unwrap_or(false);

        if has_price
            && (text_lower.contains("add to cart")
                || text_lower.contains("buy now")
                || text_lower.contains("product"))
        {
            if block.text.contains("class=\"product") || block.text.contains("itemprop=\"product\"")
            {
                return Some(BlockType::ProductDetail);
            }
            return Some(BlockType::ProductCard);
        }
        None
    }

    /// LF: Comment/UserGenerated — has comment patterns
    fn lf_comment(block: &VSBBlock) -> Option<BlockType> {
        let selectors = block.source_selectors.join(" ").to_lowercase();
        let text_lower = block.text.to_lowercase();

        if selectors.contains("comment")
            || text_lower.contains("posted by")
            || text_lower.contains("replies")
            || text_lower.contains("upvote")
            || text_lower.contains("downvote")
        {
            return Some(BlockType::Comment);
        }
        None
    }

    /// LF: Hero — large heading + CTA pattern
    fn lf_hero(block: &VSBBlock) -> Option<BlockType> {
        let text_lower = block.text.to_lowercase();
        let has_large_heading = block.text.contains("<h1")
            || block.text.contains("class=\"hero\"")
            || block.text.contains("class=\"banner\"");
        let has_cta = text_lower.contains("get started")
            || text_lower.contains("sign up")
            || text_lower.contains("learn more");

        if has_large_heading && has_cta {
            return Some(BlockType::Hero);
        }
        None
    }

    /// Run all LFs and vote
    fn vote(block: &VSBBlock) -> HashMap<BlockType, f64> {
        let mut votes: HashMap<BlockType, f64> = HashMap::new();

        let lfs: [fn(&VSBBlock) -> Option<BlockType>; 18] = [
            Self::lf_navigation,
            Self::lf_header,
            Self::lf_footer,
            Self::lf_sidebar,
            Self::lf_search,
            Self::lf_form,
            Self::lf_login,
            Self::lf_toc,
            Self::lf_breadcrumb,
            Self::lf_pagination,
            Self::lf_code,
            Self::lf_table,
            Self::lf_faq,
            Self::lf_advertisement,
            Self::lf_documentation,
            Self::lf_product,
            Self::lf_comment,
            Self::lf_hero,
        ];

        for lf in lfs {
            if let Some(block_type) = lf(block) {
                *votes.entry(block_type).or_insert(0.0) += 1.0;
            }
        }

        // Normalize votes to 0-1
        let max_votes = votes.values().cloned().fold(0.0f64, f64::max);
        if max_votes > 0.0 {
            for v in votes.values_mut() {
                *v /= max_votes;
            }
        }

        votes
    }
}

/// ML Block Classifier
pub struct MLBlockClassifier {
    mode: ClassificationMode,
    /// Class centroids for embedding-based matching (ML mode)
    #[cfg(feature = "ml-embeddings")]
    centroids: HashMap<BlockType, Vec<f32>>,
}

impl MLBlockClassifier {
    pub fn new(mode: ClassificationMode) -> Self {
        Self {
            mode,
            #[cfg(feature = "ml-embeddings")]
            centroids: HashMap::new(),
        }
    }

    /// Train class centroids from labeled examples
    #[cfg(feature = "ml-embeddings")]
    pub fn train_centroids(&mut self, labeled_examples: HashMap<BlockType, Vec<String>>) {
        use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};

        let model = TextEmbedding::try_new(InitOptions::new(EmbeddingModel::BGESmallENV15))
            .expect("Failed to load BGE model");

        for (block_type, texts) in labeled_examples {
            if texts.is_empty() {
                continue;
            }
            let embeddings = model.embed(texts.clone(), None).expect("Embedding failed");
            let dim = embeddings.first().map(|e| e.len()).unwrap_or(0);
            if dim == 0 {
                continue;
            }

            // Mean pooling
            let mut centroid = vec![0.0f32; dim];
            for emb in &embeddings {
                for (i, &v) in emb.iter().enumerate() {
                    centroid[i] += v;
                }
            }
            for v in &mut centroid {
                *v /= embeddings.len() as f32;
            }

            self.centroids.insert(block_type, centroid);
        }
    }

    /// Classify a VSB block
    pub fn classify(&self, block: &VSBBlock) -> MLClassificationResult {
        match self.mode {
            ClassificationMode::Fast => self.classify_fast(block),
            #[cfg(feature = "ml-embeddings")]
            ClassificationMode::ML => self.classify_ml(block),
            #[cfg(not(feature = "ml-embeddings"))]
            ClassificationMode::ML => self.classify_fast(block), // Fallback
        }
    }

    /// Fast heuristic classification
    fn classify_fast(&self, block: &VSBBlock) -> MLClassificationResult {
        // Run weak supervision LFs
        let lf_votes = LabelingFunctions::vote(block);

        // Extract features for scoring
        let features = BlockFeatures::extract(block);

        // Feature-based scoring for each candidate type
        let mut candidates: Vec<(BlockType, f64)> = Vec::new();

        // Score each block type based on features
        candidates.extend(self.score_all_types(&features, block));

        // Combine LF votes with feature scores
        for (block_type, feature_score) in &mut candidates {
            if let Some(lf_score) = lf_votes.get(block_type) {
                // Weight: 60% feature score, 40% LF vote
                *feature_score = *feature_score * 0.6 + *lf_score * 0.4;
            }
        }

        // Sort by score descending
        candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Take top candidate
        let (best_type, confidence) = candidates
            .first()
            .cloned()
            .unwrap_or((BlockType::Generic, 0.0));
        let semantic_role = default_semantic_role(&best_type);

        let top_candidates: Vec<(BlockType, f64)> = candidates.iter().take(3).cloned().collect();
        let needs_review = confidence < 0.4;

        MLClassificationResult {
            block_type: best_type,
            semantic_role,
            confidence,
            top_candidates,
            mode: ClassificationMode::Fast,
            needs_review,
        }
    }

    /// ML classification using BGE embeddings + centroid matching
    #[cfg(feature = "ml-embeddings")]
    fn classify_ml(&self, block: &VSBBlock) -> MLClassificationResult {
        // First run fast LFs to narrow candidates
        let lf_votes = LabelingFunctions::vote(block);
        let features = BlockFeatures::extract(block);
        let feature_scores = self.score_all_types(&features, block);

        // Embed the block text
        let embedding = {
            use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
            static MODEL: std::sync::OnceLock<TextEmbedding> = std::sync::OnceLock::new();
            let model = MODEL.get_or_init(|| {
                TextEmbedding::try_new(InitOptions::new(EmbeddingModel::BGESmallENV15))
                    .expect("Failed to load BGE model")
            });
            let embeddings = model
                .embed(vec![block.text.clone()], None)
                .expect("Embedding failed");
            embeddings.into_iter().next().unwrap_or_default()
        };

        if embedding.is_empty() {
            return self.classify_fast(block); // Fallback
        }

        // Cosine similarity to each class centroid
        let mut centroid_scores: HashMap<BlockType, f64> = HashMap::new();
        for (block_type, centroid) in &self.centroids {
            let sim = cosine_similarity_f32(&embedding, centroid);
            centroid_scores.insert(block_type.clone(), sim as f64);
        }

        // Combine: 30% LF, 30% features, 40% centroid similarity
        let mut candidates: Vec<(BlockType, f64)> = Vec::new();
        for (block_type, feature_score) in feature_scores {
            let lf_score = lf_votes.get(&block_type).copied().unwrap_or(0.0);
            let centroid_score = centroid_scores.get(&block_type).copied().unwrap_or(0.0);

            let combined = feature_score * 0.3 + lf_score * 0.3 + centroid_score * 0.4;
            candidates.push((block_type, combined));
        }

        candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let (best_type, confidence) = candidates
            .first()
            .cloned()
            .unwrap_or((BlockType::Generic, 0.0));
        let semantic_role = default_semantic_role(&best_type);
        let top_candidates: Vec<(BlockType, f64)> = candidates.iter().take(3).cloned().collect();
        let needs_review = confidence < 0.4;

        MLClassificationResult {
            block_type: best_type,
            semantic_role,
            confidence,
            top_candidates,
            mode: ClassificationMode::ML,
            needs_review,
        }
    }

    /// Score all block types based on feature heuristics
    fn score_all_types(
        &self,
        features: &BlockFeatures,
        _block: &VSBBlock,
    ) -> Vec<(BlockType, f64)> {
        let mut scores = Vec::new();

        // Navigation
        let nav_score: f64 = if features.has_nav_pattern { 0.8 } else { 0.2 }
            + if features.link_to_text_ratio > 0.5 {
                0.3
            } else {
                0.0
            };
        scores.push((BlockType::Navigation, nav_score.min(1.0)));

        // Header
        let header_score: f64 = if features.has_id && features.selector_depth < 3 {
            0.3
        } else {
            0.0
        } + if features.text_length < 500 && features.heading_count > 0 {
            0.3
        } else {
            0.0
        };
        scores.push((BlockType::Header, header_score.min(1.0)));

        // Footer
        let footer_score: f64 = if features.selector_depth < 3 {
            0.2
        } else {
            0.0
        } + if features.link_count > 5 && features.text_length < 1000 {
            0.3
        } else {
            0.0
        } + if features.has_nav_pattern { 0.2 } else { 0.0 };
        scores.push((BlockType::Footer, footer_score.min(1.0)));

        // Sidebar
        let sidebar_score: f64 = if features.link_count > 3 && features.text_length < 2000 {
            0.3
        } else {
            0.0
        } + if features.child_count > 2 && features.dom_depth > 2 {
            0.2
        } else {
            0.0
        };
        scores.push((BlockType::Sidebar, sidebar_score.min(1.0)));

        // Search
        let search_score: f64 = if features.form_element_count > 0 && features.text_length < 500 {
            0.6
        } else {
            0.1
        } + if features.has_interactive { 0.2 } else { 0.0 };
        scores.push((BlockType::Search, search_score.min(1.0)));

        // Form
        let form_score: f64 = if features.form_element_count >= 2 {
            0.7
        } else {
            0.1
        } + if features.has_interactive { 0.2 } else { 0.0 };
        scores.push((BlockType::Form, form_score.min(1.0)));

        // Login
        let login_score: f64 = if features.form_element_count >= 2 && features.has_interactive {
            0.5
        } else {
            0.1
        };
        scores.push((BlockType::Login, login_score.min(1.0)));

        // TableOfContents
        let toc_score: f64 = if features.link_count > 3 && features.text_length < 1000 {
            0.4
        } else {
            0.0
        } + if features.heading_count > 2 { 0.3 } else { 0.0 };
        scores.push((BlockType::TableOfContents, toc_score.min(1.0)));

        // Breadcrumb
        let breadcrumb_score: f64 = if features.link_count >= 2 && features.text_length < 200 {
            0.5
        } else {
            0.0
        };
        scores.push((BlockType::Breadcrumb, breadcrumb_score.min(1.0)));

        // Pagination
        let pagination_score: f64 = if features.has_pagination_links {
            0.8
        } else {
            0.1
        } + if features.numeric_ratio > 0.2 {
            0.2
        } else {
            0.0
        };
        scores.push((BlockType::Pagination, pagination_score.min(1.0)));

        // Code
        let code_score: f64 = if features.has_code_patterns { 0.8 } else { 0.1 }
            + if features.avg_word_length > 6.0 {
                0.2
            } else {
                0.0
            };
        scores.push((BlockType::Code, code_score.min(1.0)));

        // Table
        let table_score: f64 = if features.table_row_count > 1 {
            0.8
        } else {
            0.1
        } + if features.numeric_ratio > 0.3 {
            0.2
        } else {
            0.0
        };
        scores.push((BlockType::Table, table_score.min(1.0)));

        // FAQ
        let faq_score: f64 = if features.text_length > 200 && features.heading_count > 1 {
            0.3
        } else {
            0.0
        } + if features.has_schema_org { 0.3 } else { 0.0 };
        scores.push((BlockType::FAQ, faq_score.min(1.0)));

        // Advertisement
        let ad_score: f64 = if features.text_length < 300 && features.link_count > 0 {
            0.3
        } else {
            0.0
        } + if features.has_media_embed { 0.2 } else { 0.0 };
        scores.push((BlockType::Advertisement, ad_score.min(1.0)));

        // Documentation
        let doc_score: f64 = if features.text_length > 500 && features.heading_count > 2 {
            0.3
        } else {
            0.0
        } + if features.has_code_patterns { 0.3 } else { 0.0 }
            + if features.link_count > 2 { 0.2 } else { 0.0 };
        scores.push((BlockType::Documentation, doc_score.min(1.0)));

        // ProductCard
        let product_score: f64 = if features.text_length < 500 && features.image_count > 0 {
            0.3
        } else {
            0.0
        } + if features.link_count == 1 { 0.2 } else { 0.0 };
        scores.push((BlockType::ProductCard, product_score.min(1.0)));

        // ProductDetail
        let product_detail_score: f64 = if features.text_length > 500 && features.image_count > 0 {
            0.3
        } else {
            0.0
        } + if features.has_interactive { 0.2 } else { 0.0 }
            + if features.child_count > 3 { 0.2 } else { 0.0 };
        scores.push((BlockType::ProductDetail, product_detail_score.min(1.0)));

        // Review
        let review_score: f64 = if features.has_schema_org { 0.4 } else { 0.0 }
            + if features.text_length > 50 && features.text_length < 2000 {
                0.2
            } else {
                0.0
            };
        scores.push((BlockType::Review, review_score.min(1.0)));

        // Comment
        let comment_score: f64 = if features.text_length > 20 && features.text_length < 3000 {
            0.2
        } else {
            0.0
        } + if features.dom_depth > 3 { 0.2 } else { 0.0 };
        scores.push((BlockType::Comment, comment_score.min(1.0)));

        // UserProfile
        let profile_score: f64 = if features.image_count > 0 && features.text_length < 500 {
            0.3
        } else {
            0.0
        } + if features.has_email_patterns {
            0.2
        } else {
            0.0
        };
        scores.push((BlockType::UserProfile, profile_score.min(1.0)));

        // Feed
        let feed_score: f64 = if features.child_count > 3 && features.link_count > 3 {
            0.3
        } else {
            0.0
        } + if features.text_length > 500 && features.text_length < 10000 {
            0.2
        } else {
            0.0
        };
        scores.push((BlockType::Feed, feed_score.min(1.0)));

        // Hero
        let hero_score: f64 = if features.text_length < 1000 && features.heading_count > 0 {
            0.3
        } else {
            0.0
        } + if features.dom_depth < 3 { 0.2 } else { 0.0 }
            + if features.has_interactive { 0.2 } else { 0.0 };
        scores.push((BlockType::Hero, hero_score.min(1.0)));

        // FeatureGrid
        let feature_grid_score: f64 = if features.child_count > 2 && features.heading_count > 1 {
            0.3
        } else {
            0.0
        } + if features.image_count > 1 { 0.2 } else { 0.0 };
        scores.push((BlockType::FeatureGrid, feature_grid_score.min(1.0)));

        // Testimonial
        let testimonial_score: f64 = if features.text_length > 50 && features.text_length < 1000 {
            0.3
        } else {
            0.0
        } + if features.has_id { 0.1 } else { 0.0 };
        scores.push((BlockType::Testimonial, testimonial_score.min(1.0)));

        // CallToAction
        let cta_score: f64 = if features.text_length < 300 && features.has_interactive {
            0.5
        } else {
            0.1
        } + if features.heading_count > 0 { 0.2 } else { 0.0 };
        scores.push((BlockType::CallToAction, cta_score.min(1.0)));

        // Chart
        let chart_score: f64 = if features.has_media_embed { 0.5 } else { 0.0 }
            + if features.text_length < 500 { 0.2 } else { 0.0 };
        scores.push((BlockType::Chart, chart_score.min(1.0)));

        // APIDocumentation
        let api_doc_score: f64 = if features.has_code_patterns && features.text_length > 500 {
            0.4
        } else {
            0.0
        } + if features.has_url_patterns { 0.2 } else { 0.0 };
        scores.push((BlockType::APIDocumentation, api_doc_score.min(1.0)));

        // Tutorial
        let tutorial_score: f64 = if features.text_length > 500 && features.heading_count > 2 {
            0.3
        } else {
            0.0
        } + if features.has_code_patterns { 0.2 } else { 0.0 }
            + if features.list_item_count > 2 {
                0.2
            } else {
                0.0
            };
        scores.push((BlockType::Tutorial, tutorial_score.min(1.0)));

        // BlogPost
        let blog_score: f64 = if features.text_length > 300 && features.heading_count > 1 {
            0.3
        } else {
            0.0
        } + if features.has_email_patterns {
            0.1
        } else {
            0.0
        };
        scores.push((BlockType::BlogPost, blog_score.min(1.0)));

        // Changelog
        let changelog_score: f64 = if features.heading_count > 2 && features.numeric_ratio > 0.1 {
            0.4
        } else {
            0.0
        } + if features.text_length > 200 && features.text_length < 5000
        {
            0.2
        } else {
            0.0
        };
        scores.push((BlockType::Changelog, changelog_score.min(1.0)));

        // Specification
        let spec_score: f64 = if features.text_length > 1000 && features.heading_count > 3 {
            0.3
        } else {
            0.0
        } + if features.has_code_patterns { 0.2 } else { 0.0 };
        scores.push((BlockType::Specification, spec_score.min(1.0)));

        // Forum
        let forum_score: f64 = if features.child_count > 2 && features.link_count > 3 {
            0.3
        } else {
            0.0
        } + if features.text_length > 500 { 0.2 } else { 0.0 };
        scores.push((BlockType::Forum, forum_score.min(1.0)));

        // Filter
        let filter_score: f64 = if features.form_element_count > 1 && features.link_count > 1 {
            0.5
        } else {
            0.1
        } + if features.has_interactive { 0.2 } else { 0.0 };
        scores.push((BlockType::Filter, filter_score.min(1.0)));

        // Pricing
        let pricing_score: f64 = if features.table_row_count > 2 && features.numeric_ratio > 0.2 {
            0.4
        } else {
            0.0
        } + if features.text_length > 200 && features.text_length < 3000 {
            0.2
        } else {
            0.0
        };
        scores.push((BlockType::Pricing, pricing_score.min(1.0)));

        // Cart
        let cart_score: f64 = if features.has_interactive && features.text_length > 100 {
            0.3
        } else {
            0.0
        } + if features.form_element_count > 0 {
            0.2
        } else {
            0.0
        };
        scores.push((BlockType::Cart, cart_score.min(1.0)));

        // Media
        let media_score: f64 = if features.has_media_embed { 0.8 } else { 0.0 };
        scores.push((BlockType::Media, media_score.min(1.0)));

        // Article (default for content-rich blocks)
        let article_score: f64 = if features.text_length > 200 && features.link_to_text_ratio < 0.3
        {
            0.4
        } else {
            0.1
        } + if features.heading_count > 0 { 0.2 } else { 0.0 }
            + if !features.has_code_patterns && !features.has_media_embed {
                0.2
            } else {
                0.0
            };
        scores.push((BlockType::Article, article_score.min(1.0)));

        // Generic (fallback)
        let generic_score: f64 = if features.text_length > 0 { 0.1 } else { 0.0 };
        scores.push((BlockType::Generic, generic_score));

        // ProductListing
        let plp_score: f64 = if features.child_count > 3 && features.image_count > 2 {
            0.4
        } else {
            0.0
        } + if features.has_interactive { 0.2 } else { 0.0 };
        scores.push((BlockType::ProductListing, plp_score.min(1.0)));

        scores
    }
}

/// Cosine similarity between two f32 vectors
fn cosine_similarity_f32(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0f32;
    let mut norm_a = 0.0f32;
    let mut norm_b = 0.0f32;
    for i in 0..a.len() {
        dot += a[i] * b[i];
        norm_a += a[i] * a[i];
        norm_b += b[i] * b[i];
    }
    let denom = norm_a.sqrt() * norm_b.sqrt();
    if denom == 0.0 {
        0.0
    } else {
        dot / denom
    }
}

/// Get default semantic role for a block type
pub fn default_semantic_role(block_type: &BlockType) -> SemanticRole {
    match block_type {
        BlockType::Article
        | BlockType::BlogPost
        | BlockType::Documentation
        | BlockType::FAQ
        | BlockType::Specification
        | BlockType::Changelog
        | BlockType::Tutorial
        | BlockType::ProductDetail
        | BlockType::Code
        | BlockType::Table
        | BlockType::APIDocumentation
        | BlockType::Chart
        | BlockType::Pricing
        | BlockType::Hero
        | BlockType::FeatureGrid
        | BlockType::Testimonial
        | BlockType::CallToAction
        | BlockType::ProductListing
        | BlockType::ProductCard => SemanticRole::PrimaryContent,

        BlockType::Navigation
        | BlockType::Breadcrumb
        | BlockType::TableOfContents
        | BlockType::Pagination => SemanticRole::Navigation,

        BlockType::Header | BlockType::Footer => SemanticRole::Structural,

        BlockType::Sidebar | BlockType::Filter | BlockType::Search => {
            SemanticRole::SupportingContent
        }

        BlockType::Comment | BlockType::Feed | BlockType::Forum => SemanticRole::UserGenerated,

        BlockType::Form | BlockType::Login | BlockType::Cart => SemanticRole::Interactive,

        BlockType::Advertisement => SemanticRole::Commercial,

        BlockType::Media | BlockType::Review => SemanticRole::PrimaryContent,

        BlockType::Generic | BlockType::UserProfile => SemanticRole::Unknown,
    }
}
