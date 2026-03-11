# pucksdata Dockerfile — 3-stage cargo-chef build
#
# Stage 1 (chef):    Shared base alias with cargo-chef pre-installed.
# Stage 2 (planner): Generates recipe.json describing the dependency graph.
# Stage 3 (builder): Restores cached dependency layer from recipe.json,
#                    then compiles the full binary.
# Stage 4 (runtime): Minimal debian:bookworm-slim image containing only
#                    the compiled binary and ca-certificates for TLS.
#
# Source: https://github.com/LukeMathWalker/cargo-chef (canonical 3-stage pattern)

# ---- Stage 1: chef base (shared) ----
FROM lukemathwalker/cargo-chef:latest-rust-1-bookworm AS chef
WORKDIR /app

# ---- Stage 2: planner — generates dependency recipe ----
FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# ---- Stage 3: builder — compiles dependencies (cached layer) then binary ----
FROM chef AS builder
# Explicit SQLX_OFFLINE: no database available during docker build.
# The project also sets this in .cargo/config.toml, but that file is
# COPY'd after 'cargo chef cook', so we set it here too for the cook step.
ENV SQLX_OFFLINE=true
COPY --from=planner /app/recipe.json recipe.json
# This RUN layer is cached until Cargo.toml or Cargo.lock changes.
RUN cargo chef cook --release --recipe-path recipe.json
# Copy full source (including .sqlx/ offline cache) and build binary.
COPY . .
RUN cargo build --release --bin pucksdata

# ---- Stage 4: runtime — minimal image with binary only ----
FROM debian:bookworm-slim AS runtime
WORKDIR /app
# Install system CA certificates required for reqwest TLS verification
# against the NHL API. Clean apt lists in the same layer to avoid bloat.
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
# Create a non-root system user with no login shell and explicit UID
# for consistent behavior across environments.
RUN useradd -r -s /bin/false -u 10001 appuser
# Copy compiled binary from builder stage to a PATH-accessible location
# so subcommands (sync, daemon, etc.) work without a full path.
COPY --from=builder /app/target/release/pucksdata /usr/local/bin/pucksdata
USER appuser
# CRITICAL: exec-form (JSON array) — binary runs as PID 1 and receives
# SIGTERM directly from 'docker stop'. Shell-form "CMD pucksdata daemon"
# would spawn /bin/sh as PID 1, which does not forward SIGTERM to the
# binary, causing a 10-second SIGKILL timeout on every docker stop.
CMD ["pucksdata", "daemon"]
