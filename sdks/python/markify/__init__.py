"""Markify — The MIT-licensed web data layer for AI agents.

Scrape, search, extract, and structure web data — faster than anything else.

Usage:
    from markify import Markify

    client = Markify()  # No API key needed for self-hosted
    result = client.scrape("https://example.com")
    print(result.markdown)
"""

from __future__ import annotations

import asyncio
from typing import Any, List, Optional, Type, TypeVar
from dataclasses import dataclass, field

import httpx
from pydantic import BaseModel

T = TypeVar("T", bound=BaseModel)

DEFAULT_BASE_URL = "http://localhost:3000"


@dataclass
class ScrapeResult:
    """Result of a scrape operation."""

    url: str
    success: bool
    status_code: int = 0
    markdown: Optional[str] = None
    json_content: Optional[dict] = None
    metadata: Optional[dict] = None
    links: Optional[List[dict]] = None
    raw_html: Optional[str] = None
    error: Optional[str] = None
    engine: str = "http"
    fetch_ms: int = 0
    total_ms: int = 0
    cached: bool = False


class Markify:
    """Markify client for scraping URLs and extracting structured data."""

    def __init__(
        self,
        base_url: str = DEFAULT_BASE_URL,
        api_key: Optional[str] = None,
        timeout: float = 30.0,
    ):
        self.base_url = base_url.rstrip("/")
        self.timeout = timeout
        self._client = httpx.Client(
            base_url=self.base_url,
            timeout=timeout,
            headers={
                "Content-Type": "application/json",
                **({"Authorization": f"Bearer {api_key}"} if api_key else {}),
            },
        )
        self._async_client: Optional[httpx.AsyncClient] = None

    def _get_client(self) -> httpx.Client:
        return self._client

    async def _get_async_client(self) -> httpx.AsyncClient:
        if self._async_client is None:
            self._async_client = httpx.AsyncClient(
                base_url=self.base_url,
                timeout=self.timeout,
                headers={
                    "Content-Type": "application/json",
                },
            )
        return self._async_client

    def scrape(
        self,
        url: str,
        mode: str = "smart",
        formats: Optional[List[str]] = None,
        wait_for_selector: Optional[str] = None,
        force_browser: bool = False,
        include_links: bool = False,
        include_raw_html: bool = False,
    ) -> ScrapeResult:
        """Scrape a URL and return clean content.

        Args:
            url: The URL to scrape.
            mode: Extraction mode — article, full, links, images, metadata, smart.
            formats: Output formats — markdown, json, both.
            wait_for_selector: CSS selector to wait for (browser mode).
            force_browser: Force headless browser rendering.
            include_links: Include extracted links in response.
            include_raw_html: Include raw HTML in response.

        Returns:
            ScrapeResult with markdown, metadata, and structured data.
        """
        payload: dict[str, Any] = {
            "url": url,
            "mode": mode,
            "formats": formats or ["both"],
            "force_browser": force_browser,
            "include_links": include_links,
            "include_raw_html": include_raw_html,
        }
        if wait_for_selector:
            payload["wait_for_selector"] = wait_for_selector

        resp = self._get_client().post("/v1/scrape", json=payload)
        resp.raise_for_status()
        data = resp.json()

        if not data.get("success"):
            return ScrapeResult(
                url=url,
                success=False,
                error=data.get("error", "Unknown error"),
            )

        result_data = data.get("data", {})
        meta = data.get("meta", {})

        return ScrapeResult(
            url=url,
            success=True,
            status_code=result_data.get("status_code", 0),
            markdown=result_data.get("markdown"),
            json_content=result_data.get("json_content"),
            metadata=result_data.get("metadata"),
            links=result_data.get("links"),
            raw_html=result_data.get("raw_html"),
            engine=meta.get("engine", "http"),
            fetch_ms=meta.get("fetch_ms", 0),
            total_ms=meta.get("total_ms", 0),
            cached=meta.get("cached", False),
        )

    def batch(
        self,
        urls: List[str],
    ) -> List[ScrapeResult]:
        """Scrape multiple URLs in a batch.

        Args:
            urls: List of URLs to scrape (max 100).

        Returns:
            List of ScrapeResult objects.
        """
        resp = self._get_client().post("/v1/batch", json={"urls": urls})
        resp.raise_for_status()
        data = resp.json()

        results = []
        for item in data.get("results", []):
            results.append(
                ScrapeResult(
                    url=item.get("url", ""),
                    success=item.get("success", False),
                    status_code=item.get("status_code", 0),
                    markdown=item.get("markdown"),
                    metadata=item.get("metadata"),
                    error=item.get("error"),
                    engine="http",
                    fetch_ms=item.get("fetch_ms", 0),
                )
            )

        return results

    def metadata(self, url: str) -> Optional[dict]:
        """Get lightweight metadata for a URL (title, description, OG tags).

        Args:
            url: The URL to get metadata for.

        Returns:
            Dict with metadata fields.
        """
        resp = self._get_client().get("/v1/metadata", params={"url": url})
        resp.raise_for_status()
        data = resp.json()

        if data.get("success"):
            return data.get("metadata")
        return None

    def health(self) -> dict:
        """Check server health."""
        resp = self._get_client().get("/v1/health")
        resp.raise_for_status()
        return resp.json()

    async def scrape_async(
        self,
        url: str,
        mode: str = "smart",
        formats: Optional[List[str]] = None,
        wait_for_selector: Optional[str] = None,
        force_browser: bool = False,
        include_links: bool = False,
    ) -> ScrapeResult:
        """Async version of scrape()."""
        client = await self._get_async_client()
        payload: dict[str, Any] = {
            "url": url,
            "mode": mode,
            "formats": formats or ["both"],
            "force_browser": force_browser,
            "include_links": include_links,
        }
        if wait_for_selector:
            payload["wait_for_selector"] = wait_for_selector

        resp = await client.post("/v1/scrape", json=payload)
        resp.raise_for_status()
        data = resp.json()

        if not data.get("success"):
            return ScrapeResult(
                url=url,
                success=False,
                error=data.get("error", "Unknown error"),
            )

        result_data = data.get("data", {})
        meta = data.get("meta", {})

        return ScrapeResult(
            url=url,
            success=True,
            status_code=result_data.get("status_code", 0),
            markdown=result_data.get("markdown"),
            json_content=result_data.get("json_content"),
            metadata=result_data.get("metadata"),
            links=result_data.get("links"),
            engine=meta.get("engine", "http"),
            fetch_ms=meta.get("fetch_ms", 0),
            total_ms=meta.get("total_ms", 0),
            cached=meta.get("cached", False),
        )

    async def batch_async(self, urls: List[str]) -> List[ScrapeResult]:
        """Async version of batch()."""
        client = await self._get_async_client()
        resp = await client.post("/v1/batch", json={"urls": urls})
        resp.raise_for_status()
        data = resp.json()

        results = []
        for item in data.get("results", []):
            results.append(
                ScrapeResult(
                    url=item.get("url", ""),
                    success=item.get("success", False),
                    status_code=item.get("status_code", 0),
                    markdown=item.get("markdown"),
                    metadata=item.get("metadata"),
                    error=item.get("error"),
                    engine="http",
                    fetch_ms=item.get("fetch_ms", 0),
                )
            )

        return results

    def close(self):
        """Close the HTTP client."""
        self._client.close()
        if self._async_client:
            asyncio.get_event_loop().run_until_complete(self._async_client.aclose())

    # ─── Structured API (Parse.bot-style) ───────────────────────────────

    def generate_api(self, url: str, description: str = None) -> dict:
        """Generate a structured API spec from a URL.

        Args:
            url: URL to analyze.
            description: Natural language description of data to extract.

        Returns:
            Dict with API spec including endpoints and response schema.
        """
        payload = {"url": url}
        if description:
            payload["description"] = description

        resp = self._get_client().post("/v1/generate", json=payload)
        resp.raise_for_status()
        return resp.json()

    def list_apis(self) -> list:
        """List all generated API specs."""
        resp = self._get_client().get("/v1/apis")
        resp.raise_for_status()
        return resp.json().get("apis", [])

    def get_api(self, api_id: str) -> dict:
        """Get a specific API spec."""
        resp = self._get_client().get(f"/v1/apis/{api_id}")
        resp.raise_for_status()
        return resp.json().get("api")

    def execute_api(self, api_id: str, url: str = None, params: dict = None) -> dict:
        """Execute a generated API spec.

        Args:
            api_id: The API spec ID.
            url: Override URL (defaults to original).
            params: Input parameters (e.g., {"endpoint": "list_items"}).

        Returns:
            Extraction result with data.
        """
        payload = {}
        if url:
            payload["url"] = url
        if params:
            payload["params"] = params

        resp = self._get_client().post(f"/v1/apis/{api_id}/execute", json=payload)
        resp.raise_for_status()
        return resp.json().get("result")

    # ─── Neural Search (Exa) ────────────────────────────────────────────

    def neural_search(self, query: str, num_results: int = 5, scrape_results: bool = False) -> dict:
        """Neural/semantic search using Exa AI.

        Args:
            query: Search query (meaning-based, not keyword).
            num_results: Number of results.
            scrape_results: Whether to scrape each result.

        Returns:
            Search results with optional scraped content.
        """
        resp = self._get_client().post("/v1/neural-search", json={
            "query": query,
            "num_results": num_results,
            "scrape_results": scrape_results,
        })
        resp.raise_for_status()
        return resp.json()

    # ─── Cilow Export ───────────────────────────────────────────────────

    def export_cilow(self, url: str, tags: list = None, mode: str = "smart") -> dict:
        """Scrape a URL and export to Cilow's context engine.

        Args:
            url: URL to scrape.
            tags: Tags for Cilow indexing.
            mode: Extraction mode (smart, article, full).

        Returns:
            Scrape result + Cilow export result.
        """
        payload = {"url": url, "mode": mode}
        if tags:
            payload["tags"] = tags

        resp = self._get_client().post("/v1/export/cilow", json=payload)
        resp.raise_for_status()
        return resp.json()

    # ─── VSB-Graph (Asterism) ───────────────────────────────────────────

    def vsb(self, url: str, format: str = "both", index: bool = False) -> dict:
        """Segment a URL into Visual-Semantic Block Graph.

        Args:
            url: URL to segment.
            format: Output format: markdown, json, both.
            index: Whether to index blocks into BM25 + dense indices.

        Returns:
            VSB-Graph with structured blocks, markdown, and JSON.
        """
        resp = self._get_client().post("/v1/vsb", json={
            "url": url,
            "format": format,
            "index": index,
        })
        resp.raise_for_status()
        return resp.json()

    # ─── Sparse Index Search (BM25) ─────────────────────────────────────

    def search_index(self, query: str, limit: int = 10) -> dict:
        """Search indexed content using BM25 (Tantivy).

        Args:
            query: Search query.
            limit: Max results.

        Returns:
            BM25 search results with scores and snippets.
        """
        resp = self._get_client().get("/v1/search-index", params={
            "q": query,
            "limit": limit,
        })
        resp.raise_for_status()
        return resp.json()

    # ─── Dense Index Search (Neural/Cosine) ─────────────────────────────

    def neural_index(self, query: str, limit: int = 10) -> dict:
        """Search indexed content using dense vectors (cosine similarity).

        Args:
            query: Search query.
            limit: Max results.

        Returns:
            Neural search results with similarity scores.
        """
        resp = self._get_client().post("/v1/neural-index", json={
            "query": query,
            "limit": limit,
        })
        resp.raise_for_status()
        return resp.json()

    def __enter__(self):
        return self

    def __exit__(self, *args):
        self.close()
