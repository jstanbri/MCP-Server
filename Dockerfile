# ── Stage 1: build ──────────────────────────────────────────────────────────
FROM rust:1.88-slim AS builder

WORKDIR /app

# Install system libraries needed by tiberius / OpenSSL
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Cache dependency compilation: copy manifests first, then source.
COPY Cargo.toml Cargo.lock ./
# Build a dummy main so Cargo caches the dependency graph
RUN mkdir -p src && echo 'fn main() {}' > src/main.rs \
    && cargo build --release \
    && rm -rf src

# Build the real binary
COPY src ./src
RUN touch src/main.rs && cargo build --release

# ── Stage 2: runtime ────────────────────────────────────────────────────────
FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=builder /app/target/release/azure-mcp-server /usr/local/bin/azure-mcp-server

# Environment variables are injected at runtime via --env-file or -e flags.
# See .env-sample for the full list of supported variables.

ENTRYPOINT ["/usr/local/bin/azure-mcp-server"]
