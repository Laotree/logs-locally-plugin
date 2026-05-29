# ── Build stage ──────────────────────────────────────────────────────────────
FROM rust:1.87-slim AS builder

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
 && apt-get install -y sqlite3 \
 && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/llp /usr/local/bin/llp

EXPOSE 8484

CMD ["/bin/sh", "-c", "exec llp serve --port ${PORT:-8484}"]
