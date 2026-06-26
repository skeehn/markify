# Markify API Examples

Examples for all 25 endpoints in curl, Python, TypeScript, and Go.

Base URL: `http://127.0.0.1:3000`

---

## Scraping

### POST /v1/scrape

Scrape a URL and return clean Markdown.

**curl:**
```bash
curl -X POST http://127.0.0.1:3000/v1/scrape \
  -H "Content-Type: application/json" \
  -d '{"url":"https://example.com","mode":"smart","formats":["markdown"]}'
```

**Python:**
```python
from markify import Markify
client = Markify(base_url="http://127.0.0.1:3000")
result = client.scrape("https://example.com", mode="smart")
print(result.markdown[:500])
```

**TypeScript:**
```typescript
import { Markify } from 'markify';
const client = new Markify({ baseUrl: 'http://127.0.0.1:3000' });
const result = await client.scrape('https://example.com');
console.log(result.markdown);
```

**Go:**
```go
client := markify.New(markify.WithBaseURL("http://127.0.0.1:3000"))
result, err := client.Scrape(context.Background(), "https://example.com", markify.ScrapeOptions{Mode: "smart"})
fmt.Println(result.Markdown)
```

### POST /v1/batch

Scrape multiple URLs (max 100).

**curl:**
```bash
curl -X POST http://127.0.0.1:3000/v1/batch \
  -H "Content-Type: application/json" \
  -d '{"urls":["https://example.com","https://httpbin.org/html"]}'
```

### GET /v1/metadata

Get lightweight metadata for a URL.

**curl:**
```bash
curl "http://127.0.0.1:3000/v1/metadata?url=https://example.com"
```

### POST /v1/vsb

Get Visual-Semantic Block Graph (structured blocks with types and roles).

**curl:**
```bash
curl -X POST http://127.0.0.1:3000/v1/vsb \
  -H "Content-Type: application/json" \
  -d '{"url":"https://en.wikipedia.org/wiki/Web_scraping","format":"both","index":true}'
```

---

## Search

### POST /v1/search

Keyword search via Serper with optional scraping.

**curl:**
```bash
curl -X POST http://127.0.0.1:3000/v1/search \
  -H "Content-Type: application/json" \
  -d '{"query":"Rust web scraping","num_results":5,"scrape_results":true}'
```

### POST /v1/neural-search

Neural/semantic search via Exa AI.

**curl:**
```bash
curl -X POST http://127.0.0.1:3000/v1/neural-search \
  -H "Content-Type: application/json" \
  -d '{"query":"AI agent frameworks","num_results":5}'
```

### GET /v1/search-index

Search indexed content with BM25 (fielded boosts).

**curl:**
```bash
curl "http://127.0.0.1:3000/v1/search-index?q=web+scraping&limit=10"
```

### POST /v1/neural-index

Search indexed content with dense vector similarity (HNSW).

**curl:**
```bash
curl -X POST http://127.0.0.1:3000/v1/neural-index \
  -H "Content-Type: application/json" \
  -d '{"query":"web scraping techniques","limit":10}'
```

### POST /v1/hybrid-search

Hybrid search: BM25 + HNSW with RRF fusion.

**curl:**
```bash
curl -X POST http://127.0.0.1:3000/v1/hybrid-search \
  -H "Content-Type: application/json" \
  -d '{"query":"web data extraction","limit":10,"mode":"hybrid","rrf_k":60,"bm25_weight":1.0,"dense_weight":1.0}'
```

---

## Crawl

### POST /v1/crawl/start

Start a distributed crawl job.

**curl:**
```bash
curl -X POST http://127.0.0.1:3000/v1/crawl/start \
  -H "Content-Type: application/json" \
  -d '{"url":"https://example.com","max_pages":1000,"max_depth":3}'
```

### GET /v1/crawl/status

Check crawl job status.

**curl:**
```bash
curl "http://127.0.0.1:3000/v1/crawl/status?job_id=crawl-abc123"
```

### POST /v1/crawl/stop

Stop a crawl job.

**curl:**
```bash
curl -X POST http://127.0.0.1:3000/v1/crawl/stop \
  -H "Content-Type: application/json" \
  -d '{"job_id":"crawl-abc123"}'
```

### GET /v1/crawl/jobs

List all crawl jobs.

**curl:**
```bash
curl "http://127.0.0.1:3000/v1/crawl/jobs"
```

### GET /v1/crawl/results

Get crawl results for a job.

**curl:**
```bash
curl "http://127.0.0.1:3000/v1/crawl/results?job_id=crawl-abc123"
```

---

## Structured API

### POST /v1/generate

Generate an API spec from a URL.

**curl:**
```bash
curl -X POST http://127.0.0.1:3000/v1/generate \
  -H "Content-Type: application/json" \
  -d '{"url":"https://api.example.com","description":"User API"}'
```

### GET /v1/apis

List all generated API specs.

**curl:**
```bash
curl "http://127.0.0.1:3000/v1/apis"
```

### GET /v1/apis/:id

Get a specific API spec.

**curl:**
```bash
curl "http://127.0.0.1:3000/v1/apis/spec-abc123"
```

### POST /v1/apis/:id/execute

Execute an extraction.

**curl:**
```bash
curl -X POST http://127.0.0.1:3000/v1/apis/spec-abc123/execute \
  -H "Content-Type: application/json" \
  -d '{"url":"https://api.example.com/users"}'
```

---

## Integrations

### POST /v1/export/cilow

Export scraped content to Cilow memory system.

**curl:**
```bash
curl -X POST http://127.0.0.1:3000/v1/export/cilow \
  -H "Content-Type: application/json" \
  -d '{"url":"https://example.com","tags":["research","web"]}'
```

---

## Infrastructure

### GET /v1/health

Server health and telemetry.

**curl:**
```bash
curl "http://127.0.0.1:3000/v1/health"
```

Response:
```json
{
  "status": "ok",
  "service": "markify",
  "version": "0.1.0",
  "telemetry": {
    "requests": { "total": 150, "success": 148, "errors": 2 },
    "performance": { "avg_latency_ms": 45 },
    "cache": { "hits": 30, "hit_rate": 20.0 }
  }
}
```
