# Markify Architecture

## Overview

Markify is a Rust-native web data layer for AI agents. It provides scraping, search, structured extraction, and crawling — all through a single binary with 25 REST API endpoints and 12 MCP tools.

## System Diagram

```
┌─────────────────────────────────────────────────────────────────┐
│                        Markify Server                              │
│                                                                  │
│  ┌─────────────┐  ┌──────────────┐  ┌─────────────────────┐     │
│  │  HTTP Fetch │→ │ VSB-Graph     │→ │ Hybrid Search       │     │
│  │  (reqwest)  │  │ (35 types)   │  │ BM25 + HNSW + RRF  │     │
│  │  + browser  │  │ ML classify  │  │ fielded boosts     │     │
│  └─────────────┘  └──────────────┘  └─────────────────────┘     │
│         │                │                      │                │
│         ▼                ▼                      ▼                │
│  ┌─────────────┐  ┌──────────────┐  ┌─────────────────────┐     │
│  │  Crawl      │  │ Extraction   │  │ Anti-Bot Layer      │     │
│  │  Engine     │  │ Schema + LLM │  │ Proxy + Stealth     │     │
│  │  + bloom    │  │ + verify     │  │ + CAPTCHA           │     │
│  └─────────────┘  └──────────────┘  └─────────────────────┘     │
│                                                                  │
│  REST API (25 endpoints)  │  MCP Server (12 tools)  │  CLI      │
└─────────────────────────────────────────────────────────────────┘
```

## Components

### Core (`core/`)

The MIT/Apache-licensed Rust crate powering everything.

| Module | Purpose | Key Files |
|--------|---------|-----------|
| `scrape.rs` | Main scraping interface | Orchestrates fetch → extract → transform |
| `fetch/` | HTTP + browser fetching | `http.rs`, `browser.rs`, `proxy.rs` |
| `extract/` | Content extraction (5 modes) | `readability.rs`, `links.rs`, `metadata.rs` |
| `transform/` | Output formatting | Markdown + JSON conversion |
| `vsb_graph/` | Visual-Semantic Block Graph | `segmenter.rs`, `classifier.rs`, `ml_classifier.rs`, `types.rs` |
| `index/` | Search indexes | `sparse.rs` (BM25), `dense.rs` (HNSW), `hybrid.rs` (RRF) |
| `crawl/` | Distributed crawl engine | `engine.rs`, `frontier.rs`, `dedup.rs` |
| `search/` | Query understanding + re-ranking | `query_understanding.rs`, `reranker.rs` |
| `structured_api/` | API spec generation + extraction | `generator.rs`, `executor.rs`, `extraction.rs` |
| `telemetry/` | Metrics + OTel | `otel.rs` |

### Server (`server/`)

Binary crate: REST API (Axum), MCP server (rmcp), CLI (clap).

| File | Purpose |
|------|---------|
| `rest.rs` | 25 REST API route handlers |
| `mcp.rs` | 12 MCP tool definitions + handlers |
| `main.rs` | CLI entry point |

### SDKs (`sdks/`)

| SDK | Language | Methods |
|-----|----------|---------|
| `python/` | Python | 15 methods, async support |
| `typescript/` | TypeScript | 15 methods, typed |
| `go/` | Go | 15 methods, idiomatic |

## Data Flow

### Scraping Flow
```
URL → HTTP Fetch → HTML → VSB Segmentation → Block Classification → Markdown/JSON
         ↓
    Proxy Rotation (if enabled)
         ↓
    Browser Fallback (if JS needed)
```

### Search Flow
```
Query → Intent Detection → Entity Extraction → Query Rewriting
         ↓
    BM25 (fielded) + HNSW (vectors) in parallel
         ↓
    RRF Fusion (Reciprocal Rank Fusion)
         ↓
    Cross-Encoder Re-Ranking (MiniLM)
         ↓
    Ranked Results
```

### Crawl Flow
```
Seed URLs → URL Frontier (priority queue) → Dedup (bloom filter)
         ↓
    Per-Domain Rate Limiting → Fetch → Extract → Index
         ↓
    Link Discovery → Add to Frontier → Repeat
         ↓
    Change Detection → Recrawl if changed
```

## Key Design Decisions

1. **Rust everywhere** — Single binary, ~6KB memory, no GC pauses
2. **HTTP-first + browser fallback** — 80% of pages work without JS, 20% escalate
3. **VSB-Graph over flat Markdown** — 35 semantic block types with provenance tracking
4. **Hybrid search** — BM25 for exact keywords + HNSW for semantic meaning + RRF fusion
5. **MCP-first** — Designed for AI agent consumption from day one
6. **Pluggable external services** — Proxies, CAPTCHA solvers, LLM APIs behind trait interfaces

## Memory Footprint

| Component | Memory |
|-----------|--------|
| Base server | ~6KB |
| With BM25 index | ~50MB |
| With HNSW index | ~100MB |
| With BGE embeddings | ~180MB |
| With Donut VLM | ~380MB |
| Full SOTA | ~500MB |
