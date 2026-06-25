# MCP Setup Guide

Markify provides a built-in MCP (Model Context Protocol) server that gives AI agents direct access to web scraping and search capabilities.

## Available Tools

| Tool | Description |
|------|-------------|
| `markify_scrape` | Scrape a URL → clean Markdown |
| `markify_search` | Search the web → results with optional scrapinging |
| `markify_metadata` | Get URL metadata → title, OG tags, description |
| `markify_extract` | Full extraction → Markdown + metadata + links |
| `markify_batch` | Scrape multiple URLs → combined results |

## Claude Desktop

Add to `~/Library/Application Support/Claude/claude_desktop_config.json` (macOS) or `%APPDATA%\Claude\claude_desktop_config.json` (Windows):

```json
{
  "mcpServers": {
    "markify": {
      "command": "markify",
      "args": ["mcp"],
      "env": {
        "SERPER_API_KEY": "your_serper_key_here"
      }
    }
  }
}
```

Restart Claude Desktop. You should now see Markify tools available.

## Cursor

Add to `.cursor/mcp.json` in your project:

```json
{
  "mcpServers": {
    "markify": {
      "command": "markify",
      "args": ["mcp"],
      "env": {
        "SERPER_API_KEY": "your_serper_key_here"
      }
    }
  }
}
```

## Windsurf

Add to `.windsurf/mcp.json`:

```json
{
  "mcpServers": {
    "markify": {
      "command": "markify",
      "args": ["mcp"],
      "env": {
        "SERPER_API_KEY": "your_serper_key_here"
      }
    }
  }
}
```

## From Source

```bash
cd markify
cargo build --release
SERPER_API_KEY=your_key ./target/release/markify mcp
```

## From Docker

```bash
docker run -i -e SERPER_API_KEY=your_key skeehn/markify mcp
```

## Testing MCP Tools

Use `mcptool` or any MCP inspector to test:

```bash
# Test with a simple scrape
echo '{"jsonrpc":"2.0","method":"tools/call","params":{"name":"markify_scrape","arguments":{"url":"https://example.com"}},"id":1}' | SERPER_API_KEY=xxx markify mcp
```

## Environment Variables

| Variable | Description | Required |
|----------|-------------|----------|
| `SERPER_API_KEY` | Serper API key for web search | No (search disabled without) |
| `RUST_LOG` | Log level (debug, info, warn, error) | No (default: info) |

## Troubleshooting

**"Search not configured" error**: Set `SERPER_API_KEY` environment variable. Get a key at https://serper.dev/

**Connection refused**: Make sure the binary is in your PATH. Run `which markify` to verify.

**Tool not showing up**: Restart your AI client after updating the config.
