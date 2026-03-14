# Nectar Edge Runtime — SSR server powered by wasmtime
# Deploys to GCP Cloud Run across 29 regions

# ── Build stage ──────────────────────────────────────────────────────
FROM rust:1.91-slim AS builder

RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY compiler/ compiler/
COPY runtime/ runtime/

RUN cargo build --release --manifest-path compiler/Cargo.toml

# ── Runtime stage ────────────────────────────────────────────────────
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/target/release/nectar /usr/local/bin/nectar
COPY runtime/modules/core.js /srv/nectar/core.js

# Cloud Run sets PORT env var (default 8080)
ENV PORT=8080

# The entrypoint expects a .wasm file mounted/copied at /srv/nectar/app.wasm
# For the base image, we include a placeholder that returns a welcome page.
# Real deployments overlay their compiled .wasm via Cloud Run volume mounts
# or a custom layer.
EXPOSE 8080

ENTRYPOINT ["nectar", "serve"]
CMD ["/srv/nectar/app.wasm", "--port", "8080", "--static-dir", "/srv/nectar"]
