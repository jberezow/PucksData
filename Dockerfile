# Shared cargo-chef base.
FROM lukemathwalker/cargo-chef:latest-rust-1-bookworm@sha256:5a37e174ceb7ccbebd6d0024049e9308f4de399bbcabbc3c6c649c70486880d5 AS chef
WORKDIR /app

# Dependency recipe.
FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# Build dependencies separately to preserve Docker layer caching.
FROM chef AS builder
# `.cargo/config.toml` is not copied until after `cargo chef cook`.
ENV SQLX_OFFLINE=true
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
COPY . .
RUN cargo build --release --bin pucksdata

# Minimal non-root runtime.
FROM debian:bookworm-slim AS runtime
WORKDIR /app
# Keep system trust available for operational tools; the application uses bundled webpki roots.
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
RUN useradd -r -s /bin/false -u 10001 appuser
COPY --from=builder /app/target/release/pucksdata /usr/local/bin/pucksdata
USER appuser
# Exec form lets the daemon receive SIGTERM directly as PID 1.
CMD ["pucksdata", "daemon"]
