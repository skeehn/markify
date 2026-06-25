// Package markify provides a Go client for the Markify web data layer API.
//
// Markify scrapes, searches, extracts, and structures web data — all through
// a single self-hosted binary.
//
// Usage:
//
//	client := markify.New("http://localhost:3000")
//	result, err := client.Scrape(context.Background(), "https://example.com", nil)
//	if err != nil {
//	    log.Fatal(err)
//	}
//	fmt.Println(result.Markdown)
package markify

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"time"
)

const defaultBaseURL = "http://localhost:3000"
const defaultTimeout = 30 * time.Second

// Client is the Markify API client.
type Client struct {
	baseURL    string
	httpClient *http.Client
	apiKey     string
}

// Option configures a Client.
type Option func(*Client)

// WithHTTPClient sets the HTTP client.
func WithHTTPClient(c *http.Client) Option {
	return func(cl *Client) { cl.httpClient = c }
}

// WithAPIKey sets the API key for authentication.
func WithAPIKey(key string) Option {
	return func(cl *Client) { cl.apiKey = key }
}

// New creates a new Markify client.
func New(baseURL string, opts ...Option) *Client {
	if baseURL == "" {
		baseURL = defaultBaseURL
	}
	c := &Client{
		baseURL: baseURL,
		httpClient: &http.Client{
			Timeout: defaultTimeout,
		},
	}
	for _, opt := range opts {
		opt(c)
	}
	return c
}

// ─── Request Helpers ──────────────────────────────────────────────────────────

func (c *Client) do(ctx context.Context, method, path string, body, result interface{}) error {
	var reqBody io.Reader
	if body != nil {
		data, err := json.Marshal(body)
		if err != nil {
			return fmt.Errorf("marshal request: %w", err)
		}
		reqBody = bytes.NewReader(data)
	}

	req, err := http.NewRequestWithContext(ctx, method, c.baseURL+path, reqBody)
	if err != nil {
		return fmt.Errorf("create request: %w", err)
	}
	req.Header.Set("Content-Type", "application/json")
	if c.apiKey != "" {
		req.Header.Set("Authorization", "Bearer "+c.apiKey)
	}

	resp, err := c.httpClient.Do(req)
	if err != nil {
		return fmt.Errorf("do request: %w", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode >= 400 {
		bodyBytes, _ := io.ReadAll(resp.Body)
		return fmt.Errorf("API error %d: %s", resp.StatusCode, string(bodyBytes))
	}

	if result != nil {
		if err := json.NewDecoder(resp.Body).Decode(result); err != nil {
			return fmt.Errorf("decode response: %w", err)
		}
	}
	return nil
}

// ─── Data Types ───────────────────────────────────────────────────────────────

// ScrapeResult contains the result of a scrape operation.
type ScrapeResult struct {
	Success   bool        `json:"success"`
	Data      ScrapeData  `json:"data"`
	Meta      ScrapeMeta  `json:"meta"`
	Error     string      `json:"error,omitempty"`
}

// ScrapeData contains the scraped content.
type ScrapeData struct {
	Markdown  string                 `json:"markdown"`
	JSON      map[string]interface{} `json:"json"`
	Metadata  map[string]interface{} `json:"metadata"`
	Links     []interface{}          `json:"links"`
}

// ScrapeMeta contains metadata about the scrape.
type ScrapeMeta struct {
	Engine   string `json:"engine"`
	FetchMs  int    `json:"fetch_ms"`
	TotalMs  int    `json:"total_ms"`
	Cached   bool   `json:"cached"`
}

// ScrapeOptions configures a scrape request.
type ScrapeOptions struct {
	Mode           string   `json:"mode,omitempty"`
	Formats        []string `json:"formats,omitempty"`
	IncludeLinks   *bool    `json:"include_links,omitempty"`
	IncludeImages  *bool    `json:"include_images,omitempty"`
	TimeoutMs      *int     `json:"timeout_ms,omitempty"`
	ForceBrowser   *bool    `json:"force_browser,omitempty"`
}

// SearchResult contains search results.
type SearchResult struct {
	Success bool        `json:"success"`
	Query   string      `json:"query"`
	Count   int         `json:"count"`
	Results []interface{} `json:"results"`
}

// SearchOptions configures a search request.
type SearchOptions struct {
	NumResults   int  `json:"num_results,omitempty"`
	ScrapeResults *bool `json:"scrape_results,omitempty"`
}

// VSBResult contains VSB-Graph results.
type VSBResult struct {
	Success bool                   `json:"success"`
	Graph   map[string]interface{} `json:"graph"`
	Markdown string                `json:"markdown"`
	Blocks   []interface{}         `json:"blocks"`
}

// VSBOptions configures a VSB request.
type VSBOptions struct {
	Format string `json:"format,omitempty"`
	Index  *bool  `json:"index,omitempty"`
}

// HybridSearchResult contains hybrid search results.
type HybridSearchResult struct {
	Success bool        `json:"success"`
	Mode    string      `json:"mode"`
	Query   string      `json:"query"`
	Count   int         `json:"count"`
	Results []interface{} `json:"results"`
}

// HybridSearchOptions configures a hybrid search request.
type HybridSearchOptions struct {
	Limit       int     `json:"limit,omitempty"`
	Mode        string  `json:"mode,omitempty"`
	RRFK        float64 `json:"rrf_k,omitempty"`
	BM25Weight  float64 `json:"bm25_weight,omitempty"`
	DenseWeight float64 `json:"dense_weight,omitempty"`
}

// HealthResult contains health check results.
type HealthResult struct {
	Status  string                 `json:"status"`
	Service string                 `json:"service"`
	Version string                 `json:"version"`
	Telemetry map[string]interface{} `json:"telemetry"`
}

// ─── API Methods ──────────────────────────────────────────────────────────────

// Scrape scrapes a URL and returns clean Markdown.
func (c *Client) Scrape(ctx context.Context, url string, opts *ScrapeOptions) (*ScrapeResult, error) {
	body := map[string]interface{}{"url": url}
	if opts != nil {
		if opts.Mode != "" {
			body["mode"] = opts.Mode
		}
		if opts.Formats != nil {
			body["formats"] = opts.Formats
		}
		if opts.IncludeLinks != nil {
			body["include_links"] = *opts.IncludeLinks
		}
		if opts.IncludeImages != nil {
			body["include_images"] = *opts.IncludeImages
		}
		if opts.TimeoutMs != nil {
			body["timeout_ms"] = *opts.TimeoutMs
		}
		if opts.ForceBrowser != nil {
			body["force_browser"] = *opts.ForceBrowser
		}
	}

	var result ScrapeResult
	if err := c.do(ctx, "POST", "/v1/scrape", body, &result); err != nil {
		return nil, err
	}
	return &result, nil
}

// Search searches the web with optional scraping.
func (c *Client) Search(ctx context.Context, query string, opts *SearchOptions) (*SearchResult, error) {
	body := map[string]interface{}{"query": query}
	if opts != nil {
		if opts.NumResults > 0 {
			body["num_results"] = opts.NumResults
		}
		if opts.ScrapeResults != nil {
			body["scrape_results"] = *opts.ScrapeResults
		}
	}

	var result SearchResult
	if err := c.do(ctx, "POST", "/v1/search", body, &result); err != nil {
		return nil, err
	}
	return &result, nil
}

// Batch scrapes multiple URLs.
func (c *Client) Batch(ctx context.Context, urls []string) (map[string]interface{}, error) {
	var result map[string]interface{}
	if err := c.do(ctx, "POST", "/v1/batch", map[string]interface{}{"urls": urls}, &result); err != nil {
		return nil, err
	}
	return result, nil
}

// Metadata gets lightweight metadata for a URL.
func (c *Client) Metadata(ctx context.Context, url string) (map[string]interface{}, error) {
	var result map[string]interface{}
	if err := c.do(ctx, "GET", fmt.Sprintf("/v1/metadata?url=%s", url), nil, &result); err != nil {
		return nil, err
	}
	return result, nil
}

// VSB gets the Visual-Semantic Block Graph for a URL.
func (c *Client) VSB(ctx context.Context, url string, opts *VSBOptions) (*VSBResult, error) {
	body := map[string]interface{}{"url": url}
	if opts != nil {
		if opts.Format != "" {
			body["format"] = opts.Format
		}
		if opts.Index != nil {
			body["index"] = *opts.Index
		}
	}

	var result VSBResult
	if err := c.do(ctx, "POST", "/v1/vsb", body, &result); err != nil {
		return nil, err
	}
	return &result, nil
}

// HybridSearch searches with BM25 + HNSW RRF fusion.
func (c *Client) HybridSearch(ctx context.Context, query string, opts *HybridSearchOptions) (*HybridSearchResult, error) {
	body := map[string]interface{}{"query": query}
	if opts != nil {
		if opts.Limit > 0 {
			body["limit"] = opts.Limit
		}
		if opts.Mode != "" {
			body["mode"] = opts.Mode
		}
		if opts.RRFK > 0 {
			body["rrf_k"] = opts.RRFK
		}
		if opts.BM25Weight > 0 {
			body["bm25_weight"] = opts.BM25Weight
		}
		if opts.DenseWeight > 0 {
			body["dense_weight"] = opts.DenseWeight
		}
	}

	var result HybridSearchResult
	if err := c.do(ctx, "POST", "/v1/hybrid-search", body, &result); err != nil {
		return nil, err
	}
	return &result, nil
}

// NeuralSearch searches via Exa AI.
func (c *Client) NeuralSearch(ctx context.Context, query string, numResults int) (map[string]interface{}, error) {
	body := map[string]interface{}{"query": query, "num_results": numResults}
	var result map[string]interface{}
	if err := c.do(ctx, "POST", "/v1/neural-search", body, &result); err != nil {
		return nil, err
	}
	return result, nil
}

// CrawlStart starts a crawl job.
func (c *Client) CrawlStart(ctx context.Context, url string, maxPages, maxDepth int) (map[string]interface{}, error) {
	body := map[string]interface{}{
		"url":        url,
		"max_pages":  maxPages,
		"max_depth":  maxDepth,
	}
	var result map[string]interface{}
	if err := c.do(ctx, "POST", "/v1/crawl/start", body, &result); err != nil {
		return nil, err
	}
	return result, nil
}

// CrawlStatus gets the status of a crawl job.
func (c *Client) CrawlStatus(ctx context.Context, jobID string) (map[string]interface{}, error) {
	var result map[string]interface{}
	if err := c.do(ctx, "GET", fmt.Sprintf("/v1/crawl/status?job_id=%s", jobID), nil, &result); err != nil {
		return nil, err
	}
	return result, nil
}

// CrawlStop stops a crawl job.
func (c *Client) CrawlStop(ctx context.Context, jobID string) (map[string]interface{}, error) {
	var result map[string]interface{}
	if err := c.do(ctx, "POST", "/v1/crawl/stop", map[string]interface{}{"job_id": jobID}, &result); err != nil {
		return nil, err
	}
	return result, nil
}

// CrawlJobs lists all crawl jobs.
func (c *Client) CrawlJobs(ctx context.Context) (map[string]interface{}, error) {
	var result map[string]interface{}
	if err := c.do(ctx, "GET", "/v1/crawl/jobs", nil, &result); err != nil {
		return nil, err
	}
	return result, nil
}

// CrawlResults gets results for a crawl job.
func (c *Client) CrawlResults(ctx context.Context, jobID string) (map[string]interface{}, error) {
	var result map[string]interface{}
	if err := c.do(ctx, "GET", fmt.Sprintf("/v1/crawl/results?job_id=%s", jobID), nil, &result); err != nil {
		return nil, err
	}
	return result, nil
}

// GenerateAPI generates an API spec from a URL.
func (c *Client) GenerateAPI(ctx context.Context, url, description string) (map[string]interface{}, error) {
	body := map[string]interface{}{"url": url}
	if description != "" {
		body["description"] = description
	}
	var result map[string]interface{}
	if err := c.do(ctx, "POST", "/v1/generate", body, &result); err != nil {
		return nil, err
	}
	return result, nil
}

// ListAPIs lists all generated API specs.
func (c *Client) ListAPIs(ctx context.Context) (map[string]interface{}, error) {
	var result map[string]interface{}
	if err := c.do(ctx, "GET", "/v1/apis", nil, &result); err != nil {
		return nil, err
	}
	return result, nil
}

// ExportCilow exports content to Cilow.
func (c *Client) ExportCilow(ctx context.Context, url string, tags []string) (map[string]interface{}, error) {
	body := map[string]interface{}{"url": url, "tags": tags}
	var result map[string]interface{}
	if err := c.do(ctx, "POST", "/v1/export/cilow", body, &result); err != nil {
		return nil, err
	}
	return result, nil
}

// Health checks server health.
func (c *Client) Health(ctx context.Context) (*HealthResult, error) {
	var result HealthResult
	if err := c.do(ctx, "GET", "/v1/health", nil, &result); err != nil {
		return nil, err
	}
	return &result, nil
}
