# Self-Hosting Guide

Deploy Markify anywhere — single binary, Docker, or cloud.

## Requirements

- **CPU**: 1+ cores
- **RAM**: 256MB+ (Markify uses ~10-50MB idle)
- **Disk**: 50MB+ (binary is ~15MB)
- **Network**: Outbound internet access for fetching URLs

## Option 1: Single Binary (Recommended)

```bash
cargo install --path server
markify server --bind 0.0.0.0:3000
```

That's it. One binary, no dependencies, no runtime.

## Option 2: Docker

```bash
# Build
docker build -t skeehn/markify .

# Run
docker run -p 3000:3000 -e SERPER_API_KEY=xxx skeehn/markify

# Or with docker-compose
SERPER_API_KEY=xxx docker-compose up -d
```

Image size: ~80MB (Debian slim runtime).

## Option 3: Fly.io

```bash
# Install flyctl
curl -L https://fly.io/install.sh | sh

# Launch
fly launch  # Select existing Dockerfile
fly secrets set SERPER_API_KEY=your_key
fly deploy
```

Markify auto-detects the port and binds correctly on Fly.io.

## Option 4: Railway

1. Connect your GitHub repo to Railway
2. Select the `markify` repo
3. Set environment variable: `SERPER_API_KEY=xxx`
4. Deploy (Railway auto-detects the Dockerfile)

## Option 5: Kubernetes

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: markify
spec:
  replicas: 3
  selector:
    matchLabels:
      app: markify
  template:
    metadata:
      labels:
        app: markify
    spec:
      containers:
      - name: markify
        image: skeehn/markify:latest
        ports:
        - containerPort: 3000
        env:
        - name: SERPER_API_KEY
          valueFrom:
            secretKeyRef:
              name: markify-secrets
              key: serper-api-key
        - name: RUST_LOG
          value: info
        resources:
          requests:
            memory: "64Mi"
            cpu: "100m"
          limits:
            memory: "256Mi"
            cpu: "500m"
        livenessProbe:
          httpGet:
            path: /v1/health
            port: 3000
          initialDelaySeconds: 5
          periodSeconds: 30
---
apiVersion: v1
kind: Service
metadata:
  name: markify
spec:
  selector:
    app: markify
  ports:
  - port: 80
    targetPort: 3000
  type: LoadBalancer
```

## Configuration

All configuration via environment variables:

| Variable | Description | Default |
|----------|-------------|---------|
| `BIND` | Bind address | `0.0.0.0:3000` |
| `SERPER_API_KEY` | Serper API key for search | (empty) |
| `RUST_LOG` | Log level | `info` |
| `RUST_BACKTRACE` | Enable backtraces | `0` |

## Scaling

Markify is stateless — scale horizontally by adding more instances.

**Without cache sharing:** Each instance has its own in-memory cache. Fine for most use cases.

**With shared cache (Redis):** Uncomment Redis in `docker-compose.yml` and update the cache config in `core/src/cache.rs` to connect to Redis instead of moka.

## HTTPS

For production, put Markify behind a reverse proxy:

**Nginx:**
```nginx
server {
    listen 443 ssl;
    server_name markify.example.com;

    location / {
        proxy_pass http://127.0.0.1:3000;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
    }
}
```

**Caddy (automatic HTTPS):**
```
markify.example.com {
    reverse_proxy 127.0.0.1:3000
}
```

## Monitoring

The `/v1/health` endpoint returns basic stats:

```json
{
  "status": "ok",
  "version": "0.1.0",
  "cache": "Cache { entries: 42, size: 123456 }"
}
```

For production monitoring, enable structured logging:

```bash
RUST_LOG=markify=debug,info markify server
```

## Updating

```bash
# From source
git pull
cargo build --release

# Docker
docker pull skeehn/markify:latest
docker-compose up -d --force-recreate

# Fly.io
fly deploy
```
