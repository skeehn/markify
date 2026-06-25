//! Six-dimensional boilerplate classifier for VSB-Graph blocks.
//!
//! Dimensions:
//! 1. Length — boilerplate tends to be short and uniform
//! 2. Density — low text-to-HTML ratio
//! 3. Link ratio — boilerplate is link-heavy
//! 4. Structure — repetitive patterns (nav, footer, header)
//! 5. Uniqueness — boilerplate appears on many pages
//! 6. Freshness — boilerplate rarely changes

use crate::vsb_graph::types::*;

/// Six-dimensional boilerplate classification result
#[derive(Debug, Clone)]
pub struct BoilerplateResult {
    pub is_boilerplate: bool,
    pub confidence: f64,
    pub scores: DimensionScores,
}

#[derive(Debug, Clone)]
pub struct DimensionScores {
    pub length: f64,
    pub density: f64,
    pub link_ratio: f64,
    pub structure: f64,
    pub uniqueness: f64,
    pub freshness: f64,
}

/// Weights for each dimension (tuned empirically)
const WEIGHTS: DimensionScores = DimensionScores {
    length: 0.15,
    density: 0.20,
    link_ratio: 0.20,
    structure: 0.25,
    uniqueness: 0.10,
    freshness: 0.10,
};

/// Threshold above which a block is classified as boilerplate.
const BOILERPLATE_THRESHOLD: f64 = 0.6;

/// Classify a VSB block using the six-dimensional model.
///
/// The basic version uses heuristics. The advanced version would
/// cross-reference with historical data and other pages.
pub fn classify_block(block: &VSBBlock) -> BoilerplateResult {
    let text = &block.text;
    let text_len = text.len();

    // Dimension 1: Length
    let length_score = compute_length_score(text_len);

    // Dimension 2: Density
    let density_score = compute_density_score(block);

    // Dimension 3: Link ratio
    let link_ratio_score = compute_link_ratio_score(block);

    // Dimension 4: Structure
    let structure_score = compute_structure_score(block);

    // Dimension 5: Uniqueness (heuristic without cross-page data)
    let uniqueness_score = compute_uniqueness_score(block);

    // Dimension 6: Freshness (heuristic without historical data)
    let freshness_score = compute_freshness_score(block);

    // Weighted combination
    let confidence = length_score * WEIGHTS.length
        + density_score * WEIGHTS.density
        + link_ratio_score * WEIGHTS.link_ratio
        + structure_score * WEIGHTS.structure
        + uniqueness_score * WEIGHTS.uniqueness
        + freshness_score * WEIGHTS.freshness;

    BoilerplateResult {
        is_boilerplate: confidence > BOILERPLATE_THRESHOLD,
        confidence,
        scores: DimensionScores {
            length: length_score,
            density: density_score,
            link_ratio: link_ratio_score,
            structure: structure_score,
            uniqueness: uniqueness_score,
            freshness: freshness_score,
        },
    }
}

/// Dimension 1: Length — boilerplate tends to be short.
fn compute_length_score(text_len: usize) -> f64 {
    if text_len == 0 {
        return 1.0;
    }

    // Very short text is likely boilerplate (labels, nav items)
    if text_len < 30 {
        0.8
    } else if text_len < 100 {
        0.6
    } else if text_len < 500 {
        0.3
    } else {
        0.1
    }
}

/// Dimension 2: Density — boilerplate has low text-to-HTML ratio.
fn compute_density_score(block: &VSBBlock) -> f64 {
    let text_len = block.text.len();
    let html_len = block.html_fragment.as_ref().map(|h| h.len()).unwrap_or(0);

    if html_len == 0 {
        return 0.5;
    }

    let ratio = text_len as f64 / html_len as f64;

    // Low ratio = lots of HTML structure, little text = likely boilerplate
    if ratio < 0.05 {
        0.9
    } else if ratio < 0.15 {
        0.7
    } else if ratio < 0.30 {
        0.4
    } else {
        0.1
    }
}

/// Dimension 3: Link ratio — boilerplate is link-heavy.
fn compute_link_ratio_score(block: &VSBBlock) -> f64 {
    let word_count = block.text.split_whitespace().count();
    let link_count = block.links.len();

    if word_count == 0 {
        return 0.5;
    }

    let ratio = link_count as f64 / word_count as f64;

    if ratio > 0.5 {
        0.9
    } else if ratio > 0.3 {
        0.7
    } else if ratio > 0.1 {
        0.4
    } else {
        0.1
    }
}

/// Dimension 4: Structure — boilerplate uses repetitive patterns.
fn compute_structure_score(block: &VSBBlock) -> f64 {
    // Check selectors for boilerplate indicators
    let selectors_str = block.source_selectors.join(" ").to_lowercase();

    let boilerplate_indicators = [
        "nav",
        "navigation",
        "menu",
        "header",
        "footer",
        "sidebar",
        "advert",
        "ad-",
        "ad_",
        "sponsor",
        "cookie",
        "banner",
        "breadcrumb",
        "pagination",
        "related",
        "share",
        "social",
    ];

    let content_indicators = [
        "article", "content", "post", "entry", "body", "main", "story", "blog", "news", "text",
        "read",
    ];

    let mut boilerplate_count = 0;
    let mut content_count = 0;

    for indicator in &boilerplate_indicators {
        if selectors_str.contains(indicator) {
            boilerplate_count += 1;
        }
    }

    for indicator in &content_indicators {
        if selectors_str.contains(indicator) {
            content_count += 1;
        }
    }

    if boilerplate_count > 0 && content_count == 0 {
        0.9
    } else if boilerplate_count > content_count {
        0.7
    } else if content_count > boilerplate_count {
        0.1
    } else {
        0.5
    }
}

/// Dimension 5: Uniqueness — boilerplate appears on many pages.
fn compute_uniqueness_score(_block: &VSBBlock) -> f64 {
    // Without cross-page comparison, use heuristics:
    // Generic block types are more likely to be boilerplate
    0.5 // Default: unknown
}

/// Dimension 6: Freshness — boilerplate rarely changes.
fn compute_freshness_score(_block: &VSBBlock) -> f64 {
    // Without historical comparison, use heuristics:
    // Navigation/footer/footer-type blocks change rarely
    0.5 // Default: unknown
}
