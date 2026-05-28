# ── Build stage ──────────────────────────────────────────────────────────────
FROM rust:1.87-slim AS builder

RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
# Cache dependencies before copying source
RUN mkdir src && echo 'fn main(){}' > src/main.rs && cargo build --release && rm -rf src

COPY src ./src
# Touch main.rs so cargo rebuilds the binary
RUN touch src/main.rs && cargo build --release

# ── Runtime stage ─────────────────────────────────────────────────────────────
FROM debian:bookworm-slim

RUN apt-get update \
 && apt-get install -y ca-certificates sqlite3 \
 && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/llp /usr/local/bin/llp

# Data directory for persisted activity.json
RUN mkdir -p /data
ENV LLP_DATA_DIR=/data

# Default mode is the local log browser.
# Override with: docker run -e MODE=relay ...
# Relay requires: LLP_CF_WORKER_URL, LLP_CF_PUSH_TOKEN
ENV MODE=serve
EXPOSE 8484 8485

CMD ["/bin/sh", "-c", \
  "if [ \"$MODE\" = \"relay\" ]; then exec llp relay --port ${PORT:-8485}; \
   else exec llp serve --port ${PORT:-8484}; fi"]
