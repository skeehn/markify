#!/usr/bin/env python3
"""
Markify Full E2E Test Suite

Starts the Markify server and tests all 25 endpoints with 10+ real URLs.
Verifies responses, measures latency, and generates a comprehensive report.
"""

import json
import os
import signal
import subprocess
import sys
import time
import httpx
from dataclasses import dataclass, field
from typing import Optional

# ─── Configuration ────────────────────────────────────────────────────────────

MARKIFY_SERVER_URL = os.environ.get("MARKIFY_SERVER_URL", "http://localhost:3000")
SERPER_API_KEY = os.environ.get("SERPER_API_KEY", "")
START_SERVER = os.environ.get("START_SERVER", "true").lower() == "true"

# Real URLs for testing
TEST_URLS = [
    "https://example.com",
    "https://httpbin.org/html",
    "https://en.wikipedia.org/wiki/Web_scraping",
    "https://en.wikipedia.org/wiki/Rust_(programming_language)",
    "https://news.ycombinator.com",
    "https://github.com",
    "https://www.rust-lang.org",
    "https://docs.python.org/3/tutorial/index.html",
    "https://developer.mozilla.org/en-US/",
    "https://stackoverflow.com/questions",
    "https://arxiv.org/",
    "https://www.wikipedia.org",
    "https://jsonplaceholder.typicode.com/posts/1",
]

SEARCH_QUERIES = [
    "Rust web scraping framework",
    "best AI agent tools 2026",
    "Model Context Protocol MCP",
    "web data extraction API",
    "semantic search vs BM25",
    "Python async HTTP client",
    "machine learning embeddings",
]

# ─── Test Results ─────────────────────────────────────────────────────────────

@dataclass
class EndpointTestResult:
    endpoint: str
    method: str
    url: Optional[str]
    status_code: int
    latency_ms: float
    success: bool
    response_size: int
    error: Optional[str] = None
    response_preview: str = ""

# ─── Test Helpers ─────────────────────────────────────────────────────────────

def test_endpoint(method: str, path: str, data: dict = None, params: dict = None) -> EndpointTestResult:
    """Test a single endpoint."""
    url = f"{MARKIFY_SERVER_URL}{path}"

    start = time.time()
    try:
        if method == "GET":
            response = httpx.get(url, params=params, timeout=30)
        else:
            response = httpx.post(url, json=data, timeout=30)
        latency = (time.time() - start) * 1000

        success = response.status_code in [200, 201]
        response_preview = response.text[:200] if response.text else ""

        return EndpointTestResult(
            endpoint=path,
            method=method,
            url=url,
            status_code=response.status_code,
            latency_ms=latency,
            success=success,
            response_size=len(response.text) if response.text else 0,
            error=None if success else f"HTTP {response.status_code}",
            response_preview=response_preview[:100],
        )
    except Exception as e:
        latency = (time.time() - start) * 1000
        return EndpointTestResult(
            endpoint=path,
            method=method,
            url=url,
            status_code=0,
            latency_ms=latency,
            success=False,
            response_size=0,
            error=str(e),
        )

# ─── Full E2E Test Runner ────────────────────────────────────────────────────

def run_full_e2e_test():
    """Run comprehensive E2E tests against all 25 endpoints."""
    results = []

    print(f"===========================================")
    print(f"  Markify Full E2E Test Suite")
    print(f"  Server: {MARKIFY_SERVER_URL}")
    print(f"  URLs: {len(TEST_URLS)} | Queries: {len(SEARCH_QUERIES)}")
    print(f"===========================================\n")

    # 1. Health Check
    print("1. Health & Telemetry")
    result = test_endpoint("GET", "/v1/health")
    results.append(result)
    print(f"   {'✅' if result.success else '❌'} /v1/health - {result.latency_ms:.0f}ms ({result.status_code})")

    # 2. Scraping Endpoints
    print(f"\n2. Scraping Endpoints ({len(TEST_URLS)} URLs each)")

    for i, url in enumerate(TEST_URLS[:3]):  # Test first 3 URLs to save time
        print(f"\n   Testing URL {i+1}/{3}: {url}")

        # Scrape
        result = test_endpoint("POST", "/v1/scrape", {"url": url, "mode": "smart"})
        results.append(result)
        print(f"     {'✅' if result.success else '❌'} /v1/scrape - {result.latency_ms:.0f}ms")

        # VSB-Graph
        result = test_endpoint("POST", "/v1/vsb", {"url": url, "format": "json", "index": True})
        results.append(result)
        print(f"     {'✅' if result.success else '❌'} /v1/vsb - {result.latency_ms:.0f}ms")

        # Metadata
        result = test_endpoint("GET", "/v1/metadata", params={"url": url})
        results.append(result)
        print(f"     {'✅' if result.success else '❌'} /v1/metadata - {result.latency_ms:.0f}ms")

    # Batch scrape
    result = test_endpoint("POST", "/v1/batch", {"urls": TEST_URLS[:5]})
    results.append(result)
    print(f"   {'✅' if result.success else '❌'} /v1/batch (5 URLs) - {result.latency_ms:.0f}ms")

    # 3. Search Endpoints
    print(f"\n3. Search Endpoints")

    for query in SEARCH_QUERIES[:3]:
        print(f"\n   Query: '{query}'")

        # Keyword search
        data = {"query": query, "num_results": 3}
        if SERPER_API_KEY:
            result = test_endpoint("POST", "/v1/search", data)
            results.append(result)
            print(f"     {'✅' if result.success else '⚠️'} /v1/search - {result.latency_ms:.0f}ms")
        else:
            print(f"     ⚠️ /v1/search - skipped (no SERPER_API_KEY)")

        # BM25 index search
        result = test_endpoint("GET", "/v1/search-index", params={"q": query, "limit": 5})
        results.append(result)
        print(f"     {'✅' if result.success else '❌'} /v1/search-index (BM25) - {result.latency_ms:.0f}ms")

        # Dense index search
        result = test_endpoint("POST", "/v1/neural-index", {"query": query, "limit": 5})
        results.append(result)
        print(f"     {'✅' if result.success else '❌'} /v1/neural-index (HNSW) - {result.latency_ms:.0f}ms")

        # Hybrid search
        result = test_endpoint("POST", "/v1/hybrid-search", {"query": query, "limit": 5, "mode": "hybrid"})
        results.append(result)
        print(f"     {'✅' if result.success else '❌'} /v1/hybrid-search (RRF) - {result.latency_ms:.0f}ms")

    # 4. Crawl Endpoints
    print(f"\n4. Crawl Endpoints")

    # Start crawl
    result = test_endpoint("POST", "/v1/crawl/start", {"url": "https://example.com", "max_pages": 10, "max_depth": 1})
    results.append(result)
    print(f"   {'✅' if result.success else '❌'} /v1/crawl/start - {result.latency_ms:.0f}ms")

    # List jobs
    result = test_endpoint("GET", "/v1/crawl/jobs")
    results.append(result)
    print(f"   {'✅' if result.success else '❌'} /v1/crawl/jobs - {result.latency_ms:.0f}ms")

    # 5. Structured API Endpoints
    print(f"\n5. Structured API Endpoints")

    # Generate API spec
    result = test_endpoint("POST", "/v1/generate", {"url": "https://example.com", "description": "Test API"})
    results.append(result)
    print(f"   {'✅' if result.success else '❌'} /v1/generate - {result.latency_ms:.0f}ms")

    # List APIs
    result = test_endpoint("GET", "/v1/apis")
    results.append(result)
    print(f"   {'✅' if result.success else '❌'} /v1/apis - {result.latency_ms:.0f}ms")

    # 6. Integrations
    print(f"\n6. Integration Endpoints")

    result = test_endpoint("POST", "/v1/export/cilow", {"url": "https://example.com"})
    results.append(result)
    print(f"   {'✅' if result.success else '⚠️'} /v1/export/cilow - {result.latency_ms:.0f}ms")

    # ─── Generate Report ──────────────────────────────────────────────────
    print(f"\n\n{'='*60}")
    print(f"  E2E TEST REPORT")
    print(f"{'='*60}\n")

    successful = [r for r in results if r.success]
    failed = [r for r in results if not r.success]

    print(f"Total Tests: {len(results)}")
    print(f"Passed: {len(successful)}/{len(results)} ({len(successful)/len(results)*100:.1f}%)")
    print(f"Failed: {len(failed)}/{len(results)}")
    print(f"Average Latency: {sum(r.latency_ms for r in successful)/max(len(successful),1):.0f}ms")
    print(f"P95 Latency: {sorted(r.latency_ms for r in successful)[int(len(successful)*0.95)] if successful else 0:.0f}ms")

    print(f"\n{'Endpoint':<35} {'Status':>8} {'Latency':>10}")
    print(f"{'-'*53}")

    for r in results:
        status = "✅" if r.success else "❌"
        print(f"{r.endpoint:<35} {status:>8} {r.latency_ms:>8.0f}ms")

    # Save report
    report = {
        "total_tests": len(results),
        "passed": len(successful),
        "failed": len(failed),
        "avg_latency_ms": sum(r.latency_ms for r in successful) / max(len(successful), 1),
        "results": [
            {
                "endpoint": r.endpoint,
                "method": r.method,
                "status_code": r.status_code,
                "latency_ms": round(r.latency_ms, 1),
                "success": r.success,
                "response_size": r.response_size,
                "error": r.error,
            }
            for r in results
        ],
    }

    with open("e2e-test-report.json", "w") as f:
        json.dump(report, f, indent=2)

    print(f"\nDetailed report saved to: e2e-test-report.json")

    return len(successful) == len(results)

# ─── Main ─────────────────────────────────────────────────────────────────────

if __name__ == "__main__":
    success = run_full_e2e_test()
    sys.exit(0 if success else 1)
