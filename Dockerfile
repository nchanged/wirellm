# =========================================================================
# WireLLM - Multi-Stage Container Image for AMD64 & ARM64
# High-Performance LLM Circuit Router, Vault & Management Gateway
# =========================================================================

# -------------------------------------------------------------------------
# Stage 1: Frontend SPA Builder
# -------------------------------------------------------------------------
FROM node:20-alpine AS frontend-builder
WORKDIR /app/frontend

COPY frontend/package*.json ./
RUN npm ci

COPY frontend/ ./
RUN npm run build

# -------------------------------------------------------------------------
# Stage 2: Rust Release Binary Builder
# -------------------------------------------------------------------------
FROM rust:bookworm AS backend-builder
WORKDIR /app

# Cache dependencies
COPY Cargo.toml Cargo.lock ./
COPY crates/wirellm/Cargo.toml crates/wirellm/Cargo.toml
COPY crates/wirellm-core/Cargo.toml crates/wirellm-core/Cargo.toml

# Copy source code and pre-compiled frontend assets for rust-embed
COPY crates ./crates
COPY --from=frontend-builder /app/frontend/dist /app/frontend/dist

# Build optimized release binary
RUN cargo build --release -p wirellm

# -------------------------------------------------------------------------
# Stage 3: Minimal Production Runtime
# -------------------------------------------------------------------------
FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    tzdata \
    curl \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy binary from builder
COPY --from=backend-builder /app/target/release/wirellm /usr/local/bin/wirellm

# Prepare persistent state directory
RUN mkdir -p /data && chmod 777 /data

ENV WIRELLM_HOME=/data
ENV HOST=0.0.0.0
ENV PORT=8765

EXPOSE 8765

VOLUME ["/data"]

HEALTHCHECK --interval=15s --timeout=5s --start-period=5s --retries=3 \
    CMD ["/usr/local/bin/wirellm", "validate"] || exit 1

ENTRYPOINT ["/usr/local/bin/wirellm"]
CMD ["serve"]
