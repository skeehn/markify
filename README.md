# Markify

**The world's best web data layer for AI agents.**

Scrape, search, extract, and structure web data — faster than anything else. Apache 2.0-licensed, Rust-native, MCP-first. Available as a single binary or managed cloud.

[![CI](https://github.com/skeehn/markify/actions/workflows/ci.yml/badge.svg)](https://github.com/skeehn/markify/actions/workflows/ci.yml)
[![Apache-2.0](https://img.shields.io/badge/Apache--2.0-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-1.70%2B-orange.svg)](https://www.rust-lang.org/)
[![MCP](https://img.shields.io/badge/MCP-Server-purple.svg)](https://modelcontextprotocol.io/)

## Why Markify?

| | Markify | Firecrawl | Jina Reader |
|---|---------|-----------|-------------|
| **License** | **Apache 2.0** ✅ | AGPL-3.0 ⚠️ | Apache-2.0 |
| **Language** | **Rust** | Python/TS | Python |
| **MCP-First** | **✅ Primary** | ❌ Bolt-on | ❌ |
| **Self-Hosted** | **Single binary** | Docker compose | Limited |
| **HTML parse** | **~6 KB/doc (streaming)** | full-DOM buffer | full-DOM buffer |
| **Scrape** | ✅ | ✅ | ✅ |
| **Search** | **✅ keyless** (DuckDuckGo; Serper optional) | ✅ | ✅ |
| **Search+Scrape** | **One call** | Two calls | Two calls |
| **SDKs** | **Python/TS/Rust/Go** | Python/Node/Go | REST only |
| **Integrations** | **LangChain/LlamaIndex** | LangChain | LangChain |

Markify is the **only** Apache 2.0-licensed, Rust-native, MCP-first platform that gives AI agents reliable web access with AI-powered extraction — as a single binary anyone can self-host.

## Quick Start

```bash
# Install
cargo install --path server

# Start the server — scrape AND search work with NO API key
# (search uses keyless DuckDuckGo by default; reach it at http://127.0.0.1:3000)
markify server

# Optional: set SERPER_API_KEY for Google-quality search (free tier at serper.dev)
SERPER_API_KEY=your_key markify server

# MCP for Claude/Cursor (also keyless)
markify mcp
```

```bash
# Docker
docker run -p 3000:3000 skeehn/markify
```

> **No keys required.** Scraping needs nothing, and search falls back to keyless
> DuckDuckGo — so `/v1/scrape` and `/v1/search` both work out of the box. Set
> `SERPER_API_KEY` only if you want Google results instead of DuckDuckGo.

## Demo

```bash
# Run the full YC/Antler demo (3 use cases in <5 min)
MARKIFY_API_URL=http://127.0.0.1:3000 bash demo.sh
```

## API Endpoints

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/v1/scrape` | URL → clean Markdown + JSON |
| `POST` | `/v1/search` | Query → search results (optionally scraped) |
| `POST` | `/v1/batch` | Multiple URLs → batch results |
| `GET` | `/v1/metadata` | URL → OG tags, title, description |
| `GET` | `/v1/health` | Health + real-time telemetry |
| `POST` | `/api/convert` | Legacy: HTML string → Markdown |

### Example: Scrape a URL

```bash
curl -X POST http://127.0.0.1:3000/v1/scrape \
  -H "Content-Type: application/json" \
  -d '{"url":"https://en.wikipedia.org/wiki/Web_scraping","mode":"article","include_links":true}'
```

### Example: Search + Scrape in One Call

```bash
curl -X POST http://127.0.0.1:3000/v1/search \
  -H "Content-Type: application/json" \
  -d '{"query":"Rust web framework","scrape_results":true}'
```

### Example: MCP Config (Claude Desktop)

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

Now Claude/Cursor/Windsurf can use Markify tools directly: `markify_scrape`, `markify_search`, `markify_metadata`, `markify_extract`, `markify_batch`.

## SDKs

### Python

```python
from markify import Markify

client = Markify(base_url="http://127.0.0.1:3000")

# Scrape
result = client.scrape("https://example.com", mode="article")
print(result.markdown[:500])

# Search
results = client.search("Rust web framework", num_results=5)

# Batch
pages = client.batch(["https://example.com", "https://httpbin.org/html"])
```

### TypeScript

```typescript
import { Markify } from 'markify';

const client = new Markify({ baseUrl: 'http://127.0.0.1:3000' });
const result = await client.scrape('https://example.com');
console.log(result.markdown);
```

### LangChain

```python
from markify.langchain import MarkifyLoader, MarkifyTool

# Document Loader
loader = MarkifyLoader(urls=["https://example.com"], mode="article")
docs = loader.load()

# Tool (for agents)
tool = MarkifyTool(api_url="http://127.0.0.1:3000")
content = tool.invoke({"url": "https://example.com"})
```

## Extraction Modes

| Mode | What it does | Best for |
|------|-------------|----------|
| `smart` | Auto-detect content, pick best strategy | **Default — use this** |
| `article` | Main article text only (Readability) | Blog posts, news |
| `full` | Full page → Markdown | Complete conversion |
| `links` | All links with anchor text + scores | Research, mapping |
| `metadata` | OG tags, Twitter Cards, JSON-LD | Previews, SEO |

## Architecture

```
┌─────────────────────────────────────────────────┐
│                  Markify Server                  │
│                                                  │
│  ┌─────────┐  ┌──────────┐  ┌────────────────┐  │
│  │  HTTP   │  │  Parse   │  │    Extract     │  │
│  │  Fetch  │→ │  Engine  │→ │    Engine      │  │
│  │(reqwest)│  │(lol_html │  │ (Readability   │  │
│  │  +      │  │ + browser)│  │  + metadata)   │  │
│  │ browser │  └──────────┘  └────────────────┘  │
│  └─────────┘         ↓                           │
│               ┌──────────────┐                   │
│               │  Transform   │                   │
│               │  MD + JSON   │                   │
│               └──────────────┘                   │
│                                                  │
│  REST API  │  MCP Server  │  CLI  │  Telemetry  │
└─────────────────────────────────────────────────┘
```

### Key Technical Decisions

- **lol_html** (Cloudflare) — Streaming HTML parser, 5-6KB constant memory
- **dom_smoothie** — Only Readability impl that passes all correctness tests (Apache 2.0)
- **fast_html2md** — Built on lol_html, fastest HTML→MD converter
- **HTTP-first + browser fallback** — Fast for 80%, handles 20% JS-rendered
- **moka cache** — In-memory LRU with TTL, Redis upgrade path
- **Proxy partner integration** — BrightData/Oxylabs, don't build proxy infra

## Benchmark

```bash
python benchmarks/run_benchmarks.py --output results.md
```

Compare Markify vs Firecrawl vs Jina on the same URLs. Set `FIRECRAWL_API_KEY` and `JINA_API_KEY` to include competitors.

## Docs

- [Quickstart](QUICKSTART.md) — Get started in 2 minutes
- [API Reference](docs/API_EXAMPLES.md) — All endpoints and parameters
- [MCP Setup](docs/MCP_SETUP.md) — Claude, Cursor, Windsurf integration
- [Self-Hosting](INSTALL.md) — Production deployment guide

## Project Structure

```
markify/
├── core/              # Apache 2.0 licensed Rust crate (extraction engine)
│   └── src/
│       ├── scrape.rs          # Main scraping interface
│       ├── search.rs          # Serper API integration
│       ├── telemetry.rs       # Request metrics + health stats
│       ├── cache.rs           # LRU cache (moka → Redis)
│       ├── fetch/             # HTTP + browser fetching
│       ├── extract/           # Content extraction (5 modes)
│       ├── transform/         # MD + JSON output
│       └── crawl/             # Crawl engine (Phase 2)
├── server/            # Binary: REST API + MCP + CLI
├── frontend/          # React web app (Processor + Landing)
├── sdks/              # Python + TypeScript + Go SDKs
├── integrations/      # LangChain Document Loader + Tool
├── benchmarks/        # Reproducible vs Firecrawl/Jina
├── deploy/            # Docker, Fly.io, Railway
└── docs/              # Quickstart, API ref, MCP, self-hosting
```

## Roadmap

**Shipped:**
- ✅ REST API (scrape, search, batch, metadata)
- ✅ MCP Server (5 tools, stdio transport)
- ✅ Python + TypeScript + Go SDKs
- ✅ Serper search integration
- ✅ LangChain integration
- ✅ Telemetry + real-time health stats
- ✅ Docker + Fly.io deployment configs
- ✅ Benchmark tool

**Phase 2:**
- [ ] Headless browser pool (chromiumoxide compiled in)
- [ ] Distributed crawl engine (URL frontier, bloom filter, robots.txt)
- [ ] LLM-powered structured extraction (OpenAI/Anthropic schemas)
- [ ] Change monitoring + webhooks
- [ ] Cloud billing (Lago/Stripe integration)
- [ ] Enterprise tier (SSO, audit logs, SLA)

**Phase 3:**
- [ ] WebMCP bridge (native site tool support)
- [ ] Semantic crawling (LLM-scored link relevance)
- [ ] Publisher marketplace (compensate content creators)
- [ ] Agent mode (multi-step flows, login, pagination)

## License

Licensed under the Apache License, Version 2.0

**Apache 2.0** — Enterprise-friendly. No AGPL restrictions. Self-host freely.

Unlike Firecrawl (AGPL-3.0), Markify can be used in commercial products without open-sourcing your entire codebase. This is intentional: the Apache 2.0 license is our #1 selling point to enterprises whose legal teams flagged Firecrawl.

## Contributing

Pull requests welcome. See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

Built with ❤️ using Rust, React, and Tailwind CSS.
