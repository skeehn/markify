# Quickstart

Get Markify running in under 2 minutes.

## Option 1: Cargo (recommended)

```bash
cargo install --path server
```

Then run:

```bash
# Start the REST API server
SERPER_API_KEY=your_key markify server

# Start MCP server for Claude/Cursor
SERPER_API_KEY=your_key markify mcp

# One-shot scrape from CLI
markify scrape https://example.com --mode smart --format markdown
```

## Option 2: Docker

```bash
docker run -p 3000:3000 -e SERPER_API_KEY=your_key skeehn/markify
```

Or with docker-compose:

```bash
SERPER_API_KEY=your_key docker-compose up -d
```

## Option 3: From source

```bash
git clone https://github.com/skeehn/markify.git
cd markify
cargo build --release
./target/release/markify server
```

## Verify it works

```bash
curl http://127.0.0.1:3000/v1/health
```

Expected:
```json
{"status":"ok","service":"markify","version":"0.1.0"}
```

## Quick API examples

### Scrape a URL

```bash
curl -X POST http://127.0.0.1:3000/v1/scrape \
  -H "Content-Type: application/json" \
  -d '{"url":"https://en.wikipedia.org/wiki/Web_scraping","mode":"article"}'
```

### Search the web

```bash
curl -X POST http://127.0.0.1:3000/v1/search \
  -H "Content-Type: application/json" \
  -d '{"query":"Rust web framework","num_results":5,"scrape_results":true}'
```

### Get metadata

```bash
curl "http://127.0.0.1:3000/v1/metadata?url=https://github.com"
```

### Batch scrape

```bash
curl -X POST http://127.0.0.1:3000/v1/batch \
  -H "Content-Type: application/json" \
  -d '{"urls":["https://example.com","https://httpbin.org/html"]}'
```

## Use with Python

```bash
pip install -e sdks/python
```

```python
from markify import Markify

client = Markify(base_url="http://127.0.0.1:3000")

# Scrape
result = client.scrape("https://en.wikipedia.org/wiki/Web_scraping", mode="article")
print(result.markdown[:500])

# Search + scrape
results = client.batch([
    "https://example.com",
    "https://httpbin.org/html",
])
for r in results:
    print(f"{r.url}: {len(r.markdown or '')} chars")
```

## Use with TypeScript

```bash
cd sdks/typescript && npm install
```

```typescript
import { Markify } from 'markify';

const client = new Markify({ baseUrl: 'http://127.0.0.1:3000' });

const result = await client.scrape('https://en.wikipedia.org/wiki/Web_scraping');
console.log(result.markdown);
```

## MCP Setup (Claude, Cursor, Windsurf)

Add to your MCP config:

```json
{
  "mcpServers": {
    "markify": {
      "command": "markify",
      "args": ["mcp"],
      "env": {
        "SERPER_API_KEY": "your_key"
      }
    }
  }
}
```

Now Claude/Cursor can use Markify tools: `markify_scrape`, `markify_search`, `markify_metadata`, `markify_extract`, `markify_batch`.

## Next steps

- [API Reference](api-reference.md) — All endpoints and parameters
- [MCP Setup](mcp-setup.md) — Detailed MCP configuration
- [Self-Hosting](self-hosting.md) — Production deployment guide
- [Benchmarks](../benchmarks/run_benchmarks.py) — Compare vs competitors
