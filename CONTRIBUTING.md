# Contributing to Markify

Thank you for your interest! Markify is Apache 2.0 licensed and welcomes contributions.

## Getting Started

### Prerequisites
- Rust 1.70+
- Python 3.8+ (for benchmarks)
- Node.js 18+ (for frontend, optional)

### Setup
```bash
git clone https://github.com/skeehn/markify
cd markify
cargo build
cargo test
```

### Running the Server
```bash
cargo run --bin markify -- server
# Or with search
SERPER_API_KEY=xxx cargo run --bin markify -- server
```

## Development Workflow

1. **Fork** the repository
2. **Create a branch** (`git checkout -b feature/my-feature`)
3. **Make changes** + add tests
4. **Run tests** (`cargo test --lib && cargo test --test e2e`)
5. **Run clippy** (`cargo clippy -- -D warnings`)
6. **Format** (`cargo fmt --all`)
7. **Commit** with conventional commits (`feat:`, `fix:`, `docs:`)
8. **Open a PR**

## Code Style

- Follow [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
- Use `snake_case` for functions/variables, `PascalCase` for types
- Document public items with `///` doc comments
- Add tests for new functionality
- Keep functions under 100 lines when possible

## Testing

```bash
# Unit tests
cargo test --lib

# Integration tests
cargo test --test e2e -- --test-threads=1

# All tests
cargo test --all

# Benchmarks
python benchmarks/run_benchmarks.py
```

## Adding Features

### New API Endpoint
1. Add route in `server/src/rest.rs`
2. Add handler function
3. Add SDK methods (Python, TypeScript, Go)
4. Add MCP tool (if applicable)
5. Update `docs/API_EXAMPLES.md`
6. Add E2E test

### New VSB Block Type
1. Add variant to `BlockType` enum in `core/src/vsb_graph/types.rs`
2. Add classification logic in `core/src/vsb_graph/segmenter.rs`
3. Add labeling function in `core/src/vsb_graph/ml_classifier.rs`
4. Add emitter case in `core/src/vsb_graph/emitter.rs`
5. Add test

## Project Structure

```
markify/
├── core/src/           # Core Rust library
│   ├── scrape.rs       # Main scraper
│   ├── vsb_graph/      # VSB-Graph segmentation
│   ├── index/          # Search indexes (BM25, HNSW, hybrid)
│   ├── crawl/          # Distributed crawl engine
│   ├── search/         # Query understanding + re-ranking
│   └── fetch/          # HTTP + browser + proxy
├── server/src/         # Binary: REST + MCP + CLI
├── sdks/               # Python, TypeScript, Go SDKs
├── frontend/           # React web app
├── benchmarks/         # Performance comparison tool
├── docs/               # Documentation
└── tests/e2e/          # Integration tests
```

## Reporting Issues

- **Bugs**: Include steps to reproduce, expected/actual behavior, logs
- **Features**: Describe the use case and why it's valuable
- **Security**: Email skeehn@markify.dev directly (don't open a public issue)

## License

By contributing, you agree that your contributions will be licensed under Apache 2.0.
