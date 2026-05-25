# Multi-stage build for the mosaik-api daemon.
#
#   docker build -f docker/mosaik-api.Dockerfile -t mosaik-api:dev .
#
# Build context is the repo root so the whole Cargo workspace is in scope.
# Output image ships only the `mosaik-api` binary on a slim Debian base
# (~80 MB) — no Cargo toolchain, no source.

# ─── builder ─────────────────────────────────────────────────────────────────
FROM rust:1.83-bookworm AS builder

WORKDIR /usr/src/mosaik

# Workspace manifests first so dependency layers cache across source edits.
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates

# We don't need the Liquid Simplicity simulator at runtime; the API only uses
# the synchronous Elements RPC + the Tessera compiler. Cargo will still pull
# heavy deps the first time — accept the cost, the build layer is cached.
RUN cargo build --release --bin mosaik-api

# ─── runtime ─────────────────────────────────────────────────────────────────
FROM debian:bookworm-slim AS runtime

# `ca-certificates` so outbound HTTPS works; `tini` for clean signal handling
# (axum traps SIGTERM but a PID-1 init keeps the container shutdown well-behaved).
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates tini \
 && rm -rf /var/lib/apt/lists/*

# Non-root user; the API binds 8080 (>1024) so no caps needed.
RUN useradd --system --uid 10001 --home /app --shell /usr/sbin/nologin mosaik
WORKDIR /app
USER mosaik

COPY --from=builder /usr/src/mosaik/target/release/mosaik-api /usr/local/bin/mosaik-api

ENV MOSAIK_API_PORT=8080
EXPOSE 8080

HEALTHCHECK --interval=15s --timeout=3s --start-period=10s --retries=4 \
  CMD ["/usr/local/bin/mosaik-api", "--help"]
# Real readiness comes from /healthz once the orchestrator can curl us; the
# above just verifies the binary is intact (no curl in the slim image).

ENTRYPOINT ["/usr/bin/tini", "--", "/usr/local/bin/mosaik-api"]
