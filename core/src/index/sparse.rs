//! Sparse BM25 Index using Tantivy with fielded boosting.
//!
//! Provides fast full-text search across all scraped content blocks.
//! Fields are indexed separately with configurable boosts:
//! - title (boost: 3.0x) — highest signal
//! - headers (boost: 2.0x) — section headings
//! - body (boost: 1.0x) — main content
//! - metadata (boost: 1.5x) — tags, descriptions

use std::sync::Mutex;

use tantivy::{
    collector::TopDocs,
    directory::MmapDirectory,
    doc,
    query::{BoostQuery, QueryParser},
    schema::{Schema, FAST, STORED, STRING, TEXT},
    Document, Index, TantivyDocument,
};
use tracing::debug;

/// Helper: extract first string value from Tantivy JSON (handles both string and array formats)
fn extract_str(json: &serde_json::Value, key: &str) -> String {
    match json.get(key) {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(serde_json::Value::Array(arr)) => arr
            .first()
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        _ => String::new(),
    }
}

/// Search result from the sparse index
#[derive(Debug, Clone, serde::Serialize)]
pub struct SparseSearchResult {
    pub block_id: String,
    pub url: String,
    pub title: String,
    pub text_snippet: String,
    pub score: f64,
    pub block_type: String,
    pub source_url: String,
}

/// Field boost configuration
pub struct FieldBoost {
    pub field_name: &'static str,
    pub boost: f32,
}

/// Default field boosts — tuned empirically
const FIELD_BOOSTS: &[FieldBoost] = &[
    FieldBoost {
        field_name: "title",
        boost: 3.0,
    },
    FieldBoost {
        field_name: "headers",
        boost: 2.0,
    },
    FieldBoost {
        field_name: "body",
        boost: 1.0,
    },
    FieldBoost {
        field_name: "metadata",
        boost: 1.5,
    },
];

/// Schema fields
struct Fields {
    block_id: tantivy::schema::Field,
    url: tantivy::schema::Field,
    title: tantivy::schema::Field,
    headers: tantivy::schema::Field,
    body: tantivy::schema::Field,
    metadata: tantivy::schema::Field,
    block_type: tantivy::schema::Field,
    indexed_at: tantivy::schema::Field,
}

fn build_schema() -> (Schema, Fields) {
    let mut builder = Schema::builder();
    let block_id = builder.add_text_field("block_id", STRING | STORED);
    let url = builder.add_text_field("url", TEXT | STORED);
    let title = builder.add_text_field("title", TEXT | STORED);
    let headers = builder.add_text_field("headers", TEXT | STORED);
    let body = builder.add_text_field("body", TEXT | STORED);
    let metadata = builder.add_text_field("metadata", TEXT | STORED);
    let block_type = builder.add_text_field("block_type", STRING | STORED);
    let indexed_at = builder.add_date_field("indexed_at", FAST | STORED);
    let _ = builder.add_u64_field("seq", FAST);

    let schema = builder.build();
    let fields = Fields {
        block_id,
        url,
        title,
        headers,
        body,
        metadata,
        block_type,
        indexed_at,
    };
    (schema, fields)
}

/// Sparse BM25 index wrapper with fielded boosting
pub struct SparseIndex {
    index: Index,
    fields: Fields,
    _schema: Schema,
    doc_count: Mutex<u64>,
}

impl SparseIndex {
    /// Create a new in-memory index
    pub fn new_in_memory() -> anyhow::Result<Self> {
        let (schema, fields) = build_schema();
        let index = Index::builder().schema(schema.clone()).create_in_ram()?;
        Ok(Self {
            index,
            fields,
            _schema: schema,
            doc_count: Mutex::new(0),
        })
    }

    /// Create or open an index on disk
    pub fn open_or_create(path: &std::path::Path) -> anyhow::Result<Self> {
        let (schema, fields) = build_schema();
        let index = if path.exists() {
            Index::open_or_create(MmapDirectory::open(path)?, schema.clone())?
        } else {
            std::fs::create_dir_all(path)?;
            Index::create(
                MmapDirectory::open(path)?,
                schema.clone(),
                tantivy::IndexSettings::default(),
            )?
        };
        Ok(Self {
            index,
            fields,
            _schema: schema,
            doc_count: Mutex::new(0),
        })
    }

    /// Index a single content block with fielded boosts.
    ///
    /// # Arguments
    /// * `block_id` — unique identifier for the block
    /// * `url` — source URL
    /// * `title` — page/block title (high boost)
    /// * `headers` — concatenated section headers (medium boost)
    /// * `body` — main text content (base weight)
    /// * `metadata` — tags, descriptions, schema.org data (medium boost)
    /// * `block_type` — VSB block type classification
    pub fn index_block(
        &self,
        block_id: &str,
        url: &str,
        title: &str,
        headers: &str,
        body: &str,
        metadata: &str,
        block_type: &str,
    ) -> anyhow::Result<()> {
        let mut writer = self.index.writer(50_000_000)?;
        writer.add_document(doc!(
            self.fields.block_id => block_id,
            self.fields.url => url,
            self.fields.title => title,
            self.fields.headers => headers,
            self.fields.body => body,
            self.fields.metadata => metadata,
            self.fields.block_type => block_type,
            self.fields.indexed_at => tantivy::DateTime::from_timestamp_nanos(
                chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
            ),
        ))?;
        writer.commit()?;

        if let Ok(mut count) = self.doc_count.lock() {
            *count += 1;
        }

        debug!(
            block_id,
            body_len = body.len(),
            "Indexed block with fielded boosts"
        );
        Ok(())
    }

    /// Backwards-compatible index_block for legacy callers
    pub fn index_block_simple(
        &self,
        block_id: &str,
        url: &str,
        title: &str,
        text: &str,
        block_type: &str,
    ) -> anyhow::Result<()> {
        self.index_block(block_id, url, title, "", text, "", block_type)
    }

    /// Build a boosted query from a search string.
    /// Uses DisjunctionMaxQuery semantics via BoostQuery composition.
    fn build_boosted_query(&self, query_str: &str) -> Box<dyn tantivy::query::Query> {
        use tantivy::query::Query;

        // Parse the query for each field and apply boosts
        let mut boosted_subqueries: Vec<(Box<dyn Query>, f32)> = Vec::new();

        for field_boost in FIELD_BOOSTS {
            let field = match field_boost.field_name {
                "title" => self.fields.title,
                "headers" => self.fields.headers,
                "body" => self.fields.body,
                "metadata" => self.fields.metadata,
                _ => continue,
            };

            let _field_name = match field_boost.field_name {
                "title" => "title",
                "headers" => "headers",
                "body" => "body",
                "metadata" => "metadata",
                _ => continue,
            };

            let qp = QueryParser::for_index(&self.index, vec![field]);
            if let Ok(subquery) = qp.parse_query(query_str) {
                let boosted = BoostQuery::new(subquery, field_boost.boost);
                boosted_subqueries.push((Box::new(boosted), field_boost.boost));
            }
        }

        // If no subqueries matched, fall back to searching all text fields
        if boosted_subqueries.is_empty() {
            let qp = QueryParser::for_index(
                &self.index,
                vec![
                    self.fields.title,
                    self.fields.headers,
                    self.fields.body,
                    self.fields.metadata,
                ],
            );
            if let Ok(query) = qp.parse_query(query_str) {
                return query;
            }
        }

        // Combine with additive scoring (sum of boosted subqueries)
        // This gives us DisjunctionMax-like behavior with proper boosting
        let mut queries: Vec<Box<dyn Query>> =
            boosted_subqueries.into_iter().map(|(q, _)| q).collect();

        if queries.len() == 1 {
            return queries.pop().unwrap();
        }

        // Use SumQuery (via BooleanQuery with all Should clauses)
        let occs: Vec<(tantivy::query::Occur, Box<dyn Query>)> = queries
            .into_iter()
            .map(|q| (tantivy::query::Occur::Should, q))
            .collect();

        Box::new(tantivy::query::BooleanQuery::from(occs))
    }

    /// Search the index using BM25 with fielded boosts
    pub fn search(&self, query: &str, limit: usize) -> anyhow::Result<Vec<SparseSearchResult>> {
        let reader = self.index.reader()?;
        let searcher = reader.searcher();

        let tantivy_query = self.build_boosted_query(query);

        let top_docs = searcher.search(&*tantivy_query, &TopDocs::with_limit(limit))?;

        let mut results = Vec::new();
        for (score, doc_address) in top_docs {
            let doc = searcher.doc::<TantivyDocument>(doc_address)?;

            // Use to_json for reliable field extraction
            let json_str = doc.to_json(&self._schema);
            let json: serde_json::Value = serde_json::from_str(&json_str).unwrap_or_default();

            let block_id = extract_str(&json, "block_id");
            let url = extract_str(&json, "url");
            let title = extract_str(&json, "title");
            let body = extract_str(&json, "body");
            let block_type = extract_str(&json, "block_type");

            // Use body for snippet (prefer body, fall back to title)
            let snippet_text = if !body.is_empty() {
                body
            } else {
                title.clone()
            };
            let snippet = if snippet_text.len() > 200 {
                format!("{}...", &snippet_text[..200])
            } else {
                snippet_text
            };

            results.push(SparseSearchResult {
                block_id,
                url: url.clone(),
                title,
                text_snippet: snippet,
                score: score as f64,
                block_type,
                source_url: url,
            });
        }

        debug!(
            query,
            results = results.len(),
            "BM25 fielded search complete"
        );
        Ok(results)
    }

    /// Get total searchable document count
    pub fn doc_count(&self) -> u64 {
        self.doc_count.lock().map(|c| *c).unwrap_or(0)
    }
}
