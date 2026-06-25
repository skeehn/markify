# Installing Markify

Three ways to install Markify — pick the one that fits your workflow.

## Option 1: Cargo (Rust developers)

```bash
# Requires Rust 1.70+
cargo install --path server

# Start the server
markify server

# Or with API keys
SERPER_API_KEY=your_key markify server
```

## Option 2: Docker (Everyone)

```bash
# Clone the repo
git clone https://github.com/skeehn/markify
cd markify

# Build and run
docker compose up -d

# Test it works
curl http://localhost:3000/v1/health

# With API keys
docker compose --env-file .env up -d
```

## Option 3: Homebrew (macOS)

```bash
# Tap the repo
brew tap skeehn/markify

# Install
brew install markify

# Start
markify server
```

## Verify Installation

```bash
# Check version
markify --version

# Start server
markify server &

# Health check
curl http://localhost:3000/v1/health

# Scrape a URL
curl -X POST http://localhost:3000/v1/scrape \
  -H "Content-Type: application/json" \
  -d '{"url":"https://example.com","mode":"smart"}'
```

## Configuration

### Environment Variables

| Variable | Required | Description |
|----------|----------|-------------|
| `SERPER_API_KEY` | Yes (for search) | Get from [serper.dev](https://serper.dev) |
| `EXA_API_KEY` | No | For neural search via Exa |
| `PORT` | No | Server port (default: 3000) |
| `RUST_LOG` | No | Log level (default: info) |

### Config File

Create `.env` in the project root:

```env
SERPER_API_KEY=your_key_here
EXA_API_KEY=optional_key_here
PORT=3000
RUST_LOG=info
```

## System Requirements

- **CPU**: x86_64 or aarch64 (Apple Silicon supported)
- **RAM**: 512MB minimum, 1GB recommended
- **Disk**: 100MB for binary, 500MB with models
- **OS**: macOS, Linux, Windows (WSL2)

## Troubleshooting

### Build fails

```bash
# Clean and rebuild
cargo clean
cargo build --release
```

### Port already in use

```bash
# Use a different port
PORT=8080 markify server
```

### Docker build fails

```bash
# Clear Docker cache
docker system prune -f
docker compose build --no-cache
```
