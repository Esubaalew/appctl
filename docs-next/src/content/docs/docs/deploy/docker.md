---
title: Docker
description: Build a minimal container image for appctl serve.
---

There is no official image yet. A minimal one is straightforward.

## Dockerfile

```dockerfile
FROM rust:1.82-slim AS build
WORKDIR /src
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev nodejs npm && rm -rf /var/lib/apt/lists/*
COPY . .
RUN cd web && npm ci && npm run build
RUN cargo install --locked --path crates/appctl

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates libssl3 && rm -rf /var/lib/apt/lists/*
COPY --from=build /usr/local/cargo/bin/appctl /usr/local/bin/appctl
RUN useradd -m -s /bin/bash appctl
USER appctl
WORKDIR /home/appctl
EXPOSE 4242
ENTRYPOINT ["appctl"]
CMD ["serve", "--bind", "0.0.0.0", "--port", "4242", "--strict"]
```

Build and run:

```bash
docker build -t appctl:0.2 .
docker run --rm -p 4242:4242 \
  -e APPCTL_TOKEN="$(openssl rand -hex 32)" \
  -v "$PWD/.appctl:/home/appctl/.appctl:ro" \
  appctl:0.2 serve --bind 0.0.0.0 --port 4242 --token "$APPCTL_TOKEN" --strict
```

Mount `.appctl/` read-only so containers cannot overwrite your schema.

## docker-compose

```yaml
services:
  appctl:
    image: appctl:0.2
    ports: ["4242:4242"]
    environment:
      APPCTL_TOKEN: ${APPCTL_TOKEN}
    volumes:
      - ./.appctl:/home/appctl/.appctl:ro
    command:
      - serve
      - --bind=0.0.0.0
      - --port=4242
      - --token=${APPCTL_TOKEN}
      - --strict
      - --confirm
    restart: unless-stopped
```

## Secrets inside the container

The OS keychain is not available inside most containers. Use environment variables for every `api_key_ref`:

```bash
docker run ... \
  -e ANTHROPIC_API_KEY="$ANTHROPIC_API_KEY" \
  -e SUPABASE_ANON_KEY="$SUPABASE_ANON_KEY" \
  appctl:0.2
```

See [Secrets and keys](/docs/deploy/secrets-and-keys/).

## See also

- [Server deployment](/docs/deploy/server/)
- [Secrets and keys](/docs/deploy/secrets-and-keys/)
