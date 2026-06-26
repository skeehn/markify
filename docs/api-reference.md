# API Reference

Base URL: `http://127.0.0.1:3000`

## Endpoints

### POST /v1/scrape

Scrape a URL and return clean Markdown + structured JSON.

**Request:**

```json
{
  "url": "https://example.com/article",
  "mode": "smart",
  "formats": ["markdown", "json"],
  "wait_for_selector": null,
  "timeout_ms": 30000,
  "force_browser": false,
  "include_raw_html": false,
  "include_links": true,
  "include_images": false
}
```

**Parameters:**

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `url` | string | required | URL to scrape |
| `mode` | string | `"smart"` | Extraction mode: `smart`, `article`, `full`, `links`, `images`, `metadata` |
| `formats` | string[] | `["both"]` | Output formats: `markdown`, `json`, `both` |
| `wait_for_selector` | string | null | CSS selector to wait for (browser mode) |
| `timeout_ms` | number | 30000 | Request timeout in milliseconds |
| `force_browser` | boolean | false | Force headless browser rendering |
| `include_raw_html` | boolean | false | Include raw HTML in response |
| `include_links` | boolean | false | Include extracted links |
| `include_images` | boolean | false | Include extracted images |

**Response:**

```json
{
  "success": true,
  "data": {
    "url": "https://example.com",
    "final_url": "https://example.com/",
    "status_code": 200,
    "markdown": "# Example Domain\n\nContent...",
    "json_content": { ... },
    "extracted": { "title": "Example Domain", ... },
    "metadata": { "title": "Example Domain", "language": "en", ... },
    "links": [ { "text": "Learn more", "url": "https://iana.org", "score": 0.5 } ]
  },
  "meta": {
    "cached": false,
    "engine": "http",
    "fetch_ms": 117,
    "extract_ms": 15,
    "total_ms": 133
  }
}
```

### POST /v1/search

Search the web using Serper API. Optionally scrape each result.

**Request:**

```json
{
  "query": "Rust web scraping framework",
  "num_results": 5,
  "scrape_results": false
}
```

**Parameters:**

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `query` | string | required | Search query |
| `num_results` | number | 5 | Number of results (1-10) |
| `scrape_results` | boolean | false | Scrape each result's content |

**Response (scrape_results=false):**

```json
{
  "success": true,
  "query": "Rust web scraping framework",
  "count": 5,
  "results": [
    {
      "title": "Rust web scraping: Complete guide",
      "link": "https://www.scrapingbee.com/blog/web-scraping-rust/",
      "snippet": "...",
      "position": 1
    }
  ]
}
```

### GET /v1/metadata

Get lightweight metadata for a URL.

**Request:**

```
GET /v1/metadata?url=https://github.com
```

**Response:**

```json
{
  "success": true,
  "url": "https://github.com",
  "metadata": {
    "title": "GitHub · Change is constant...",
    "description": "Join the world's most widely adopted...",
    "image": "https://images.ctfassets.net/...",
    "site_name": "GitHub",
    "language": "en",
    "canonical_url": "https://github.com",
    "twitter_card": "summary_large_image"
  }
}
```

### POST /v1/batch

Scrape multiple URLs (max 100).

**Request:**

```json
{
  "urls": [
    "https://example.com",
    "https://httpbin.org/html"
  ]
}
```

**Response:**

```json
{
  "success": true,
  "total": 2,
  "results": [
    {
      "url": "https://example.com",
      "success": true,
      "status_code": 200,
      "markdown": "This domain is for...",
      "metadata": { ... },
      "fetch_ms": 117,
      "engine": "http"
    }
  ]
}
```

### GET /v1/health

Health check with cache stats.

**Response:**

```json
{
  "status": "ok",
  "service": "markify",
  "version": "0.1.0",
  "cache": "Cache { entries: 0, size: 0 }"
}
```

### POST /api/convert

Legacy endpoint: Convert raw HTML string to Markdown.

**Request:**

```json
{ "html": "<h1>Hello</h1><p>World</p>" }
```

**Response:**

```json
{ "success": true, "markdown": "# Hello\n\nWorld" }
```

## Extraction Modes

| Mode | Description | Best for |
|------|-------------|----------|
| `smart` | Auto-detect content type, pick best strategy | Default — use this |
| `article` | Extract main article only (Readability) | Blog posts, news articles |
| `full` | Convert entire page to Markdown | Complete page conversion |
| `links` | Extract all links with anchor text | Link mapping, research |
| `images` | Extract all images | Media extraction |
| `metadata` | OG tags, title, description only | Lightweight previews |

## Error Responses

All endpoints return consistent error format:

```json
{
  "success": false,
  "error": "Description of what went wrong"
}
```

Common status codes:
- `400` — Bad request (missing URL, invalid input)
- `500` — Internal server error
- `503` — Service unavailable (search not configured)
