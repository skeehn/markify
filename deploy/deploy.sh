#!/bin/bash
# Markify Fly.io Deployment Script
#
# Usage:
#   ./deploy/deploy.sh              # Full deploy
#   ./deploy/deploy.sh --no-deploy  # Just create app, don't deploy
#
# Prerequisites:
#   - flyctl installed: curl -L https://fly.io/install.sh | sh
#   - PATH updated: export PATH="$HOME/.fly/bin:$PATH"
#   - Logged in: flyctl auth login

set -euo pipefail

APP_NAME="${APP_NAME:-markify}"
REGION="${REGION:-sjc}"

echo "============================================"
echo "  Markify Fly.io Deployment"
echo "============================================"
echo ""
echo "App: $APP_NAME"
echo "Region: $REGION"
echo ""

# Check flyctl
if ! command -v flyctl &> /dev/null; then
    echo "ERROR: flyctl not found. Install with:"
    echo "  curl -L https://fly.io/install.sh | sh"
    echo "  export PATH=\"\$HOME/.fly/bin:\$PATH\""
    exit 1
fi

# Check auth
if ! flyctl auth whoami &> /dev/null 2>&1; then
    echo "Not logged in. Opening browser for authentication..."
    flyctl auth login
fi

# Create app (skip if exists)
if ! flyctl status --app "$APP_NAME" &> /dev/null 2>&1; then
    echo "Creating app: $APP_NAME..."
    flyctl apps create "$APP_NAME" --org personal
    echo ""
    echo "Setting up fly.toml..."
    flyctl launch --name "$APP_NAME" --region "$REGION" --no-deploy --internal-port 3000 --copy-config
else
    echo "App $APP_NAME already exists, skipping creation."
fi

# Provision PostgreSQL
echo ""
echo "Checking PostgreSQL..."
if ! flyctl postgres list 2>/dev/null | grep -q "${APP_NAME}-db"; then
    echo "Creating PostgreSQL database..."
    flyctl postgres create --name "${APP_NAME}-db" --region "$REGION" --initial-cluster-size 1 --vm-size shared-cpu-1x --volume-size 1
else
    echo "Database ${APP_NAME}-db already exists."
fi

# Attach PostgreSQL
echo ""
echo "Attaching database..."
flyctl postgres attach --postgres-app "${APP_NAME}-db" --app "$APP_NAME" || echo "Database already attached."

# Provision Redis (Upstash)
echo ""
echo "Setting up Redis..."
echo "  Create an Upstash Redis cluster at https://upstash.com"
echo "  Then set the REDIS_URL secret:"
echo "  flyctl secrets set REDIS_URL=redis://... --app $APP_NAME"

# Set required secrets
echo ""
echo "Setting required secrets..."

if [ -z "${SERPER_API_KEY:-}" ]; then
    echo "Enter your SERPER_API_KEY (get from https://serper.dev):"
    read -s SERPER_API_KEY
fi
flyctl secrets set SERPER_API_KEY="$SERPER_API_KEY" --app "$APP_NAME"

# Deploy
if [[ "${1:-}" != "--no-deploy" ]]; then
    echo ""
    echo "Deploying..."
    flyctl deploy --app "$APP_NAME" --remote-only

    echo ""
    echo "Waiting for health check..."
    sleep 15

    # Health check
    APP_URL="https://${APP_NAME}.fly.dev"
    echo ""
    echo "Testing health endpoint..."
    if curl -sf "$APP_URL/v1/health" > /dev/null 2>&1; then
        echo "  SUCCESS: $APP_URL/v1/health"
        echo ""
        echo "Markify is live at: $APP_URL"
        echo ""
        echo "Run the cloud validation:"
        echo "  ./deploy/test-cloud.sh $APP_URL"
    else
        echo "  WARNING: Health check failed. Check logs:"
        echo "  flyctl logs --app $APP_NAME"
    fi
else
    echo ""
    echo "App created. Deploy when ready:"
    echo "  flyctl deploy --app $APP_NAME"
fi

echo ""
echo "============================================"
echo "  Useful Commands"
echo "============================================"
echo "  flyctl status --app $APP_NAME"
echo "  flyctl logs --app $APP_NAME"
echo "  flyctl secrets list --app $APP_NAME"
echo "  flyctl scale count 2 --app $APP_NAME"
echo "  curl https://${APP_NAME}.fly.dev/v1/health"
