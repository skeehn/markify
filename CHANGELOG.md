# Changelog

All notable changes to Markify will be documented in this file.

## [0.1.0] — 2026-04-09

### Added
- **VSB-Graph** with 35 semantic block types and ML classifier (BGE + 18 labeling functions)
- **Hybrid Search**: BM25 (Tantivy, fielded boosts) + HNSW (usearch) + RRF fusion
- **Query Understanding**: Intent detection, entity extraction, spell/abbreviation rewriting
- **Cross-Encoder Re-Ranking**: MiniLM with heuristic fallback
- **Distributed Crawl Engine**: URL frontier, bloom filter, change detection, checkpointing
- **Enterprise Anti-Bot**: Proxy rotation, stealth fingerprints, CAPTCHA solving, bot detection
- **Structured Extraction**: Schema + LLM modes with program synthesis and verification
- **12 MCP Tools**: scrape, search, metadata, extract, batch, vsb, hybrid_search, crawl_start, crawl_status, extract_schema, neural_search, health
- **25 REST API Endpoints**: Scraping, search, crawl, structured, integrations, infra
- **Python + TypeScript + Go SDKs**
- **better-auth** multi-tenant setup with API keys, roles, 2FA
- **OpenTelemetry** observability with distributed tracing
- **Benchmark Tool** vs Firecrawl, Jina, Parse.bot, Spider

### Performance
- ~6KB base memory footprint
- 8-10x faster than Python alternatives
- Sub-200ms average scrape latency

### License
- Apache 2.0 (open source)
