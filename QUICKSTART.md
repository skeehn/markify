# Markify Quickstart

Get Markify running in 3 minutes. No experience required.

## Step 1: Install (30 seconds)

```bash
git clone https://github.com/skeehn/markify
cd markify
cargo build --release
```

Or with Docker:
```bash
git clone https://github.com/skeehn/markify
cd markify
docker compose up -d
```

## Step 2: Start the Server (10 seconds)

```bash
# With search (recommended)
SERPER_API_KEY=your_key markify server

# Without search (scraping only)
markify server
```

You'll see:
```
Markify server starting on http://0.0.0.0:3000
```

## Step 3: Scrape Your First URL (30 seconds)

```bash
curl -X POST http://127.0.0.1:3000/v1/scrape \
  -H "Content-Type: application/json" \
  -d '{"url":"https://example.com","mode":"smart"}'
```

Response:
```json
{
  "success": true,
  "data": {
    "markdown": "# Example Domain\n\nThis domain is for use in...",
    "metadata": { "title": "Example Domain", "language": "en" }
  },
  "meta": { "engine": "http", "fetch_ms": 45 }
}
```

## Step 4: Search the Web (30 seconds)

```bash
curl -X POST http://127.0.0.1:3000/v1/search \
  -H "Content-Type: application/json" \
  -d '{"query":"Rust programming","num_results":5,"scrape_results":true}'
```

## Step 5: Get Structured Blocks (30 seconds)

```bash
curl -X POST http://127.0.0.1:3000/v1/vsb \
  -H "Content-Type: application/json" \
  -d '{"url":"https://en.wikipedia.org/wiki/Web_scraping","format":"both"}'
```

Returns the page segmented into semantic blocks (article, navigation, table, code, etc.) with provenance tracking.

## Step 6: Hybrid Search (30 seconds)

```bash
# Index some content first
curl -X POST http://127.0.0.1:3000/v1/vsb \
  -H "Content-Type: application/json" \
  -d '{"url":"https://example.com","index":true}'

# Then search with BM25 + HNSW fusion
curl -X POST http://127.0.0.1:3000/v1/hybrid-search \
  -H "Content-Type: application/json" \
  -d '{"query":"example domain","limit":5,"mode":"hybrid"}'
```

## What's Next?

- **[API Reference](docs/API_REFERENCE.md)** — All 25 endpoints documented
- **[API Examples](docs/API_EXAMPLES.md)** — curl, Python, TypeScript, Go examples
- **[Architecture](docs/ARCHITECTURE.md)** — How Markify works internally
- **[MCP Setup](docs/mcp-setup.md)** — Connect to Claude, Cursor, Windsurf

## Quick Commands Reference

| Command | What it does |
|---------|-------------|
| `markify server` | Start HTTP server on port 3000 |
| `markify mcp` | Start MCP server for AI agents |
| `curl /v1/health` | Check server health |
| `curl /v1/scrape` | Scrape a URL |
| `curl /v1/search` | Search the web |
| `curl /v1/vsb` | Get structured blocks |
| `curl /v1/hybrid-search` | Hybrid BM25 + vector search |
| `curl /v1/crawl/start` | Start a crawl job |
