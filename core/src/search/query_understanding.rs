//! Query Understanding Pipeline.
//!
//! Interprets user intent and normalizes queries before search.
//!
//! Stages:
//! 1. **Intent Detection**: Classify query as navigational/informational/transactional/entity
//! 2. **Entity Extraction**: NER for dates, ranges, units, domains
//! 3. **Spell & Abbreviation Expansion**: Generate candidate rewrites
//! 4. **Query Rewriting**: Fast rule-based → slow LLM cascade

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ─── Intent Detection ────────────────────────────────────────────────────────

/// Query intent classification
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum QueryIntent {
    /// User wants a specific page/site ("markify.com", "github login")
    Navigational,
    /// User wants information ("how to scrape websites", "rust tutorial")
    Informational,
    /// User wants to perform an action ("buy domain", "sign up")
    Transactional,
    /// User is looking for an entity ("Elon Musk", "Python 3.12")
    EntitySearch,
    /// Unknown intent
    Unknown,
}

/// Intent detection result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentResult {
    pub intent: QueryIntent,
    pub confidence: f64,
    pub query: String,
}

/// Lightweight intent classifier using keyword patterns and heuristics
pub struct IntentClassifier {
    navigational_patterns: Vec<Regex>,
    transactional_patterns: Vec<Regex>,
    entity_patterns: Vec<Regex>,
}

impl Default for IntentClassifier {
    fn default() -> Self {
        Self::new()
    }
}

impl IntentClassifier {
    pub fn new() -> Self {
        Self {
            navigational_patterns: vec![
                Regex::new(r"(?i)\b(site|website|homepage|login|signin|signup)\b").unwrap(),
                Regex::new(r"(?i)\b[A-Za-z0-9.-]+\.[a-z]{2,}(/\S*)?\b").unwrap(), // URL-like
            ],
            transactional_patterns: vec![Regex::new(
                r"(?i)\b(buy|download|install|register|subscribe|order)\b",
            )
            .unwrap()],
            entity_patterns: vec![
                Regex::new(r"(?i)\b(who|what|when|where)\s+(is|was|are)\b").unwrap(),
                Regex::new(r"^[A-Z][a-z]+\s+[A-Z][a-z]+$").unwrap(), // Capitalized names
            ],
        }
    }

    /// Classify a query's intent
    pub fn classify(&self, query: &str) -> IntentResult {
        let query_lower = query.to_lowercase();

        // Check navigational patterns
        for pattern in &self.navigational_patterns {
            if pattern.is_match(query) {
                return IntentResult {
                    intent: QueryIntent::Navigational,
                    confidence: 0.8,
                    query: query.to_string(),
                };
            }
        }

        // Check transactional patterns
        for pattern in &self.transactional_patterns {
            if pattern.is_match(query) {
                return IntentResult {
                    intent: QueryIntent::Transactional,
                    confidence: 0.8,
                    query: query.to_string(),
                };
            }
        }

        // Check entity patterns
        for pattern in &self.entity_patterns {
            if pattern.is_match(query) {
                return IntentResult {
                    intent: QueryIntent::EntitySearch,
                    confidence: 0.7,
                    query: query.to_string(),
                };
            }
        }

        // Check for question words (informational)
        let question_words = ["how", "what", "why", "when", "where", "who", "which"];
        if question_words.iter().any(|w| query_lower.starts_with(w)) {
            return IntentResult {
                intent: QueryIntent::Informational,
                confidence: 0.75,
                query: query.to_string(),
            };
        }

        // Default: informational for most search queries
        IntentResult {
            intent: QueryIntent::Informational,
            confidence: 0.5,
            query: query.to_string(),
        }
    }
}

// ─── Entity Extraction ──────────────────────────────────────────────────────

/// Extracted entity from a query
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedEntity {
    pub entity_type: EntityType,
    pub value: String,
    pub span: (usize, usize),
}

/// Types of entities that can be extracted
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum EntityType {
    Date,
    DateRange,
    Number,
    Unit,
    Domain,
    Language,
    FileType,
}

/// Entity extraction result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityExtractionResult {
    pub entities: Vec<ExtractedEntity>,
    pub cleaned_query: String,
}

/// Extract entities from a query
pub fn extract_entities(query: &str) -> EntityExtractionResult {
    let mut entities = Vec::new();

    // Date patterns: 2024-01-01, Jan 2024, Q1 2024
    let date_re =
        Regex::new(r"\b(\d{4}-\d{2}-\d{2}|[A-Z][a-z]{2}\s+\d{4}|Q[1-4]\s+\d{4})\b").unwrap();
    for mat_ in date_re.find_iter(query) {
        entities.push(ExtractedEntity {
            entity_type: EntityType::Date,
            value: mat_.as_str().to_string(),
            span: (mat_.start(), mat_.end()),
        });
    }

    // Number + unit patterns: 100ms, 5GB, 10px
    let number_unit_re =
        Regex::new(r"\b(\d+(?:\.\d+)?)\s*(ms|GB|MB|KB|px|em|rem|kg|cm|m)\b").unwrap();
    for mat_ in number_unit_re.find_iter(query) {
        entities.push(ExtractedEntity {
            entity_type: EntityType::Unit,
            value: mat_.as_str().to_string(),
            span: (mat_.start(), mat_.end()),
        });
    }

    // Domain patterns: example.com, *.example.com
    let domain_re = Regex::new(r"\b(?:\*\.|[a-zA-Z0-9-]+\.)+[a-z]{2,}\b").unwrap();
    for mat_ in domain_re.find_iter(query) {
        entities.push(ExtractedEntity {
            entity_type: EntityType::Domain,
            value: mat_.as_str().to_string(),
            span: (mat_.start(), mat_.end()),
        });
    }

    // File type patterns: filetype:pdf, ext:docx
    let filetype_re = Regex::new(r"(?i)(?:filetype|ext):(\w+)").unwrap();
    for mat_ in filetype_re.find_iter(query) {
        entities.push(ExtractedEntity {
            entity_type: EntityType::FileType,
            value: mat_.as_str().to_string(),
            span: (mat_.start(), mat_.end()),
        });
    }

    // Language patterns: in:python, lang:rust
    let lang_re = Regex::new(r"(?i)(?:in|lang):(\w+)").unwrap();
    for mat_ in lang_re.find_iter(query) {
        entities.push(ExtractedEntity {
            entity_type: EntityType::Language,
            value: mat_.as_str().to_string(),
            span: (mat_.start(), mat_.end()),
        });
    }

    // Clean query by removing entity-specific modifiers
    let cleaned = filetype_re.replace_all(query, "").trim().to_string();
    let cleaned = lang_re.replace_all(&cleaned, "").trim().to_string();

    EntityExtractionResult {
        entities,
        cleaned_query: if cleaned.is_empty() {
            query.to_string()
        } else {
            cleaned
        },
    }
}

// ─── Query Rewriting ────────────────────────────────────────────────────────

/// Query rewrite result
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct QueryRewriteResult {
    pub original: String,
    pub rewritten: String,
    pub confidence: f64,
    pub rewrite_type: RewriteType,
}

/// Type of rewrite applied
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub enum RewriteType {
    /// No rewrite needed
    #[default]
    None,
    /// Spell correction applied
    SpellCorrection,
    /// Abbreviation expansion applied
    AbbreviationExpansion,
    /// Synonym expansion applied
    SynonymExpansion,
    /// Intent-based rewrite applied
    IntentRewrite,
}

/// Common abbreviation expansions
fn get_abbreviation_expansions() -> HashMap<&'static str, &'static str> {
    let mut map = HashMap::new();
    map.insert("js", "javascript");
    map.insert("ts", "typescript");
    map.insert("py", "python");
    map.insert("rs", "rust");
    map.insert("rb", "ruby");
    map.insert("go", "golang");
    map.insert("ml", "machine learning");
    map.insert("ai", "artificial intelligence");
    map.insert("api", "application programming interface");
    map.insert("url", "uniform resource locator");
    map.insert("html", "hypertext markup language");
    map.insert("css", "cascading style sheets");
    map
}

/// Common misspellings
fn get_spell_corrections() -> HashMap<&'static str, &'static str> {
    let mut map = HashMap::new();
    map.insert("scrap", "scrape");
    map.insert("scrapping", "scraping");
    map.insert("weeb", "web");
    map.insert("parsing", "parsing");
    map.insert("extraxt", "extract");
    map.insert("extrac", "extract");
    map.insert("markdwon", "markdown");
    map.insert("jsnon", "json");
    map
}

/// Rewrite a query using rule-based expansions
pub fn rewrite_query(query: &str) -> QueryRewriteResult {
    let abbreviations = get_abbreviation_expansions();
    let spell_corrections = get_spell_corrections();

    let mut rewritten = query.to_string();
    let mut rewrite_type = RewriteType::None;
    let mut confidence = 1.0;

    // Spell correction
    for (wrong, correct) in &spell_corrections {
        if rewritten.to_lowercase().contains(wrong) {
            rewritten = rewritten.replace(wrong, correct);
            rewrite_type = RewriteType::SpellCorrection;
            confidence = 0.8;
        }
    }

    // Abbreviation expansion
    for (abbrev, expansion) in &abbreviations {
        // Match whole word only
        let pattern = format!(r"\b{}\b", regex::escape(abbrev));
        if let Ok(re) = Regex::new(&pattern) {
            if re.is_match(&rewritten.to_lowercase()) {
                rewritten = re.replace_all(&rewritten, *expansion).to_string();
                rewrite_type = RewriteType::AbbreviationExpansion;
                confidence = 0.7;
            }
        }
    }

    QueryRewriteResult {
        original: query.to_string(),
        rewritten,
        confidence,
        rewrite_type,
    }
}

// ─── Full Pipeline ──────────────────────────────────────────────────────────

/// Complete query understanding pipeline result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryUnderstandingResult {
    pub original_query: String,
    pub intent: IntentResult,
    pub entities: EntityExtractionResult,
    pub rewrite: QueryRewriteResult,
    /// Final query to use for search
    pub final_query: String,
}

/// Run the full query understanding pipeline
pub fn understand_query(query: &str) -> QueryUnderstandingResult {
    let classifier = IntentClassifier::new();
    let intent = classifier.classify(query);
    let entities = extract_entities(query);
    let rewrite = rewrite_query(&entities.cleaned_query);

    // If rewrite changed the query, use the rewritten version
    let final_query = if rewrite.rewrite_type != RewriteType::None && rewrite.confidence > 0.6 {
        rewrite.rewritten.clone()
    } else {
        entities.cleaned_query.clone()
    };

    QueryUnderstandingResult {
        original_query: query.to_string(),
        intent,
        entities,
        rewrite,
        final_query,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_intent_navigational() {
        let classifier = IntentClassifier::new();
        let result = classifier.classify("login to github");
        assert_eq!(result.intent, QueryIntent::Navigational);
    }

    #[test]
    fn test_intent_informational() {
        let classifier = IntentClassifier::new();
        let result = classifier.classify("how to scrape websites");
        assert_eq!(result.intent, QueryIntent::Informational);
    }

    #[test]
    fn test_entity_extraction() {
        let result = extract_entities("scrape example.com filetype:pdf");
        assert_eq!(result.entities.len(), 2);
        assert_eq!(result.entities[0].entity_type, EntityType::Domain);
        assert_eq!(result.entities[1].entity_type, EntityType::FileType);
    }

    #[test]
    fn test_query_rewrite_abbreviations() {
        let result = rewrite_query("best js framework");
        assert!(result.rewritten.contains("javascript"));
        assert_eq!(result.rewrite_type, RewriteType::AbbreviationExpansion);
    }

    #[test]
    fn test_query_rewrite_spell_correction() {
        let result = rewrite_query("web scrap tool");
        assert!(result.rewritten.contains("scrape"));
        assert_eq!(result.rewrite_type, RewriteType::SpellCorrection);
    }

    #[test]
    fn test_full_pipeline() {
        let result = understand_query("best js web scrap framework");
        assert!(!result.final_query.is_empty());
        // Should expand "js" → "javascript" and correct "scrap" → "scrape"
        assert!(result.final_query.to_lowercase().contains("javascript"));
        assert!(result.final_query.to_lowercase().contains("scrape"));
    }
}
