#!/bin/bash
# Markify Master Test Runner
#
# Runs:
# 1. Rust unit tests (61 tests)
# 2. Full E2E tests with real URLs (25 endpoints x 10+ URLs)
# 3. AI model tests (7 models x 12 prompts = 84 tests)
#
# Usage:
#   ./run_all_tests.sh
#   OPENROUTER_API_KEY=xxx CEREBRAS_API_KEY=xxx ./run_all_tests.sh

set -euo pipefail

cd "$(dirname "$0")/.."

echo "==========================================="
echo "  MARKIFY MASTER TEST SUITE"
echo "==========================================="
echo ""

# 1. Rust Unit Tests
echo "1. Running Rust Unit Tests..."
echo "   cargo test --lib"
cargo test --lib 2>&1 | tail -5
echo ""

# 2. Rust E2E Tests
echo "2. Running Rust E2E Tests..."
echo "   cargo test --test e2e -- --test-threads=1"
cargo test --test e2e -- --test-threads=1 2>&1 | tail -5
echo ""

# 3. Python Full E2E Tests (requires running server)
echo "3. Running Full E2E Tests with Real URLs..."
echo "   NOTE: Server must be running at http://localhost:3000"
echo ""

# Check if server is running
if curl -sf http://localhost:3000/v1/health > /dev/null 2>&1; then
    echo "   Server is running, starting E2E tests..."
    python3 tests/test_full_e2e.py
else
    echo "   ⚠️ Server not running. Skipping Python E2E tests."
    echo "   Start server with: cargo run --bin markify -- server"
    echo "   Then run: python3 tests/test_full_e2e.py"
fi

echo ""

# 4. AI Model Tests
echo "4. Running AI Model Tests..."
if [ -z "${OPENROUTER_API_KEY:-}" ] || [ -z "${CEREBRAS_API_KEY:-}" ]; then
    echo "   ⚠️ API keys not set. Skipping AI model tests."
    echo "   Set: OPENROUTER_API_KEY and CEREBRAS_API_KEY"
    echo "   Then run: cd markify-sota && python3 tests/test_ai_models.py"
else
    cd ../markify-sota
    python3 tests/test_ai_models.py
    cd ../markify
fi

echo ""
echo "==========================================="
echo "  ALL TESTS COMPLETE"
echo "==========================================="
echo ""
echo "Reports generated:"
echo "  - e2e-test-report.json (endpoint tests with real URLs)"
echo "  - ai-model-test-report.json (AI model latency/quality)"
echo ""
echo "To deploy to fly.io:"
echo "  cd markify && ./deploy/deploy.sh"
