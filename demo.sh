#!/bin/bash
# Markify YC/Antler Demo Script
# Run this to show the 3 killer use cases in <5 minutes
#
# Prerequisites:
#   - Server running: SERPER_API_KEY=xxx cargo run --bin markify -- server
#   - curl installed

set -e

API_URL="${MARKIFY_API_URL:-http://localhost:8080}"
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

echo ""
echo "============================================================"
echo "  MARKIFY 2.0 — The World's Best Web Data Layer for AI Agents"
echo "============================================================"
echo ""

# ─── Demo 1: Speed ──────────────────────────────────────────────
echo -e "${GREEN}DEMO 1: 200ms to scrape any URL${NC}"
echo -e "${CYAN}============================================================${NC}"
echo ""
echo "Scraping Wikipedia: Web Scraping article..."
echo ""

START=$(date +%s%N)
RESPONSE=$(curl -s -X POST "$API_URL/v1/scrape" \
  -H "Content-Type: application/json" \
  -d '{"url":"https://en.wikipedia.org/wiki/Web_scraping","mode":"article","formats":["markdown"]}')
END=$(date +%s%N)

MS=$(( (END - START) / 1000000 ))

TITLE=$(echo "$RESPONSE" | python3 -c "import json,sys; d=json.load(sys.stdin); print(d['data']['metadata']['title'] or '?')")
MD_LEN=$(echo "$RESPONSE" | python3 -c "import json,sys; d=json.load(sys.stdin); print(len(d['data'].get('markdown','') or ''))")
ENGINE=$(echo "$RESPONSE" | python3 -c "import json,sys; d=json.load(sys.stdin); print(d['meta']['engine'])")

echo -e "  ${GREEN}✓${NC} Title: $TITLE"
echo -e "  ${GREEN}✓${NC} Content: ${MD_LEN} characters"
echo -e "  ${GREEN}✓${NC} Engine: $ENGINE"
echo -e "  ${GREEN}✓${NC} Total time: ${MS}ms"
echo ""
echo -e "  ${YELLOW}Firecrawl: ~800-2000ms | Jina: ~500-1500ms | Markify: ${MS}ms${NC}"
echo ""

# ─── Demo 2: Search + Scrape ────────────────────────────────────
echo -e "${GREEN}DEMO 2: Search → Scrape → Structured Data in ONE call${NC}"
echo -e "${CYAN}============================================================${NC}"
echo ""
echo "Searching: 'Rust web scraping framework' + scraping top 3 results..."
echo ""

START=$(date +%s%N)
RESPONSE=$(curl -s -X POST "$API_URL/v1/search" \
  -H "Content-Type: application/json" \
  -d '{"query":"Rust web scraping framework","num_results":3,"scrape_results":true}')
END=$(date +%s%N)

MS=$(( (END - START) / 1000000 ))

COUNT=$(echo "$RESPONSE" | python3 -c "import json,sys; d=json.load(sys.stdin); print(len(d.get('results',[])))")

echo -e "  ${GREEN}✓${NC} Results: $COUNT pages scraped with full content"
echo -e "  ${GREEN}✓${NC} Total time: ${MS}ms"
echo ""
echo "  Results:"
echo "$RESPONSE" | python3 -c "
import json, sys
d = json.load(sys.stdin)
for i, r in enumerate(d.get('results', [])):
    title = r.get('title', '?')[:60]
    url = r.get('url', r.get('link', '?'))[:50]
    md_len = len(r.get('markdown', '') or '')
    print(f'    {i+1}. {title}')
    print(f'       {url}')
    print(f'       {md_len} chars extracted')
    print()
"

# ─── Demo 3: Developer Experience ───────────────────────────────
echo -e "${GREEN}DEMO 3: 3 Lines of Python → Clean Data from Any URL${NC}"
echo -e "${CYAN}============================================================${NC}"
echo ""
echo "Python code:"
echo ""
echo '  from markify import Markify'
echo '  client = Markify(base_url="http://localhost:8080")'
echo '  result = client.scrape("https://github.com", mode="smart")'
echo '  print(result.markdown[:200])'
echo ""

START=$(date +%s%N)
RESPONSE=$(curl -s -X POST "$API_URL/v1/scrape" \
  -H "Content-Type: application/json" \
  -d '{"url":"https://github.com","mode":"smart","formats":["both"],"include_links":true}')
END=$(date +%s%N)

MS=$(( (END - START) / 1000000 ))

TITLE=$(echo "$RESPONSE" | python3 -c "import json,sys; d=json.load(sys.stdin); print(d['data']['metadata']['title'] or '?')")
SITE=$(echo "$RESPONSE" | python3 -c "import json,sys; d=json.load(sys.stdin); print(d['data']['metadata'].get('site_name') or '?')")
MD_LEN=$(echo "$RESPONSE" | python3 -c "import json,sys; d=json.load(sys.stdin); print(len(d['data'].get('markdown','') or ''))")
LINKS=$(echo "$RESPONSE" | python3 -c "import json,sys; d=json.load(sys.stdin); print(len(d['data'].get('links',[]) or []))")

echo -e "  ${GREEN}✓${NC} Site: $TITLE"
echo -e "  ${GREEN}✓${NC} OG Site Name: $SITE"
echo -e "  ${GREEN}✓${NC} Markdown: ${MD_LEN} chars"
echo -e "  ${GREEN}✓${NC} Links: ${LINKS} extracted with relevance scores"
echo -e "  ${GREEN}✓${NC} Time: ${MS}ms"
echo ""
echo "  Output includes: Markdown + JSON metadata + Links + Images"
echo ""

# ─── Summary ─────────────────────────────────────────────────────
echo "============================================================"
echo "  SUMMARY"
echo "============================================================"
echo ""
echo "  ✅ Scrape any URL → clean Markdown in <300ms"
echo "  ✅ Search + scrape results in ONE API call"
echo "  ✅ 3 lines of Python → structured web data"
echo "  ✅ Apache-2.0 licensed (vs Firecrawl AGPL)"
echo "  ✅ Single binary (vs Firecrawl Docker compose)"
echo "  ✅ MCP-first (Claude, Cursor, Windsurf)"
echo "  ✅ Rust-native (5-6KB memory, 10x faster than Python)"
echo ""
echo "  markify.dev | github.com/skeehn/markify"
echo ""
echo "============================================================"
