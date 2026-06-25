# Competitive Test Plan: Markify vs Firecrawl vs Jina

## Goal
Prove Markify is SOTA — 10x faster, more features, more reliable.

## Setup

### API Keys (stored in env vars)
```bash
export MARKIFY_API_URL="http://localhost:8080"
export FIRECRAWL_API_KEY="fc-a98a621a33c5434aa327f23d7941fa7f"
export JINA_API_KEY="jina_1d8ae227dba74ecfa0454655f09913ebEbp2JgQ4v2CBQCO8jddFagyZZhm9"
```

### Test Runner
```bash
python benchmarks/run_benchmarks.py
```

## Test Categories

### 1. Speed (Latency)
**URLs to test:**
| URL | Type | Why |
|-----|------|-----|
| https://example.com | Static HTML | Baseline — simplest page |
| https://en.wikipedia.org/wiki/Web_scraping | Article | Content-heavy, structured |
| https://httpbin.org/html | Static | Clean HTML, no clutter |
| https://news.ycombinator.com | Dynamic | HN, JS-rendered elements |
| https://github.com | SPA | Complex SPA, needs rendering |
| https://docs.python.org/3/tutorial/index.html | Documentation | Multi-section docs |
| https://medium.com/@example/article | Blog | Medium's complex rendering |
| https://arxiv.org/abs/2301.00001 | Academic | ArXiv's specific structure |

**Metrics:** P50, P95, P99 latency; total time for 10 URLs

### 2. Content Quality
**For each URL, measure:**
- Markdown length (chars)
- Article content extracted (vs total page content) — ratio
- Readability score (Flesch-Kincaid of output)
- Links extracted with scores
- Metadata completeness (OG title, description, image, site_name)
- Presence of nav/sidebar/ads (negative — should be stripped)

### 3. Feature Completeness
| Feature | Markify | Firecrawl | Jina |
|---------|---------|-----------|------|
| URL scrape | ✅ | ✅ | ✅ |
| Search | ✅ | ✅ (v2) | ✅ |
| Search + scrape in one call | ✅ | ❌ (2 calls) | ❌ (2 calls) |
| Article extraction | ✅ | ✅ | ❌ |
| Link extraction with scores | ✅ | ❌ | ❌ |
| Metadata extraction | ✅ | Partial | Partial |
| Batch scrape | ✅ | ✅ | ❌ |
| MCP tools | ✅ 5 tools | ❌ | ❌ |
| Output formats (MD + JSON) | ✅ | Partial | MD only |
| Self-hosted single binary | ✅ | ❌ (Docker) | ❌ |
| SDKs (Python/TS/Rust) | ✅ 3 | ✅ 3 | ❌ (REST only) |
| LangChain integration | ✅ | ✅ | ✅ |
| License | **MIT** | AGPL ⚠️ | Apache |

### 4. Reliability
**Test:** Scrape each URL 5 times, measure success rate and variance

### 5. Search Comparison
**Queries:**
- "Rust web scraping framework"
- "best AI agent tools 2026"
- "Model Context Protocol MCP"
- "web data extraction API"
- "Python LLM framework"

**Metrics:** Results returned, relevance, search+scrape time

### 6. Developer Experience
**Time to first result:**
```bash
# Markify: curl localhost:8080/v1/scrape -d '{"url":"https://example.com"}'
# Firecrawl: curl api.firecrawl.dev/v2/scrape -H "Authorization: Bearer xxx" -d '{"url":"https://example.com"}'
# Jina: curl r.jina.ai/https://example.com -H "Authorization: Bearer xxx"
```

**Time to integrated (LangChain):**
```python
# Markify: from markify.langchain import MarkifyLoader; docs = MarkifyLoader(urls=[url]).load()
# Firecrawl: from langchain_community.document_loaders import FireCrawlLoader; ...
# Jina: No native loader — manual HTTP
```

## Running the Test

```bash
# Install dependencies
pip install httpx

# Run full benchmark (Markify only)
python benchmarks/run_benchmarks.py --output results.md

# Run with all competitors
export FIRECRAWL_API_KEY="fc-xxx"
export JINA_API_KEY="jina_xxx"
python benchmarks/run_benchmarks.py --output results.md

# Custom URLs
python benchmarks/run_benchmarks.py \
  --urls https://example.com https://github.com \
  --output custom_results.md
```

## Expected Results

Based on architecture analysis:
- **Markify**: ~50-300ms per static page, ~500-2000ms for JS-heavy
- **Firecrawl**: ~800-3000ms per page (Python overhead + queue)
- **Jina**: ~500-1500ms per page (ML model inference)

Markify should be **2-10x faster** on static pages, **2-5x faster** on JS-heavy pages (when browser is compiled in).

## Reporting

The benchmark tool generates a markdown report with:
- Latency comparison table
- Content quality metrics
- Feature comparison matrix
- Per-URL breakdown

## Notes

- Don't test until user confirms ready
- Run on same machine/network for all three
- Warm up caches before testing
- Run 5 iterations per URL, use median
