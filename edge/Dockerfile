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
FROM debian:trixie-slim

RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/target/release/nectar /usr/local/bin/nectar
COPY runtime/modules/core.js /srv/nectar/core.js

# Cloud Run sets PORT env var (default 8080)
ENV PORT=8080

# The entrypoint starts in standby mode if no .wasm exists.
# Users deploy their app.wasm via Cloud Run volume mounts or a derived image.
EXPOSE 8080

ENTRYPOINT ["nectar", "serve"]
CMD ["/srv/nectar/app.wasm", "--port", "8080", "--static-dir", "/srv/nectar"]
