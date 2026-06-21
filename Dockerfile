# syntax=docker/dockerfile:1

# ---- builder ----
FROM rust:1.89-slim-bookworm AS builder
WORKDIR /app

# Build deps. rustls uses `ring`, so no OpenSSL/system TLS is required.
RUN apt-get update \
    && apt-get install -y --no-install-recommends pkg-config \
    && rm -rf /var/lib/apt/lists/*

COPY . .
# Build the node with the `quic` feature so the real QUIC transport + echo
# server are available inside the container (loopback works either way).
RUN cargo build --release -p quicforge-node --features quic

# ---- runtime ----
FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd -r -u 10001 quicforge

WORKDIR /app
COPY --from=builder /app/target/release/quicforge-node /usr/local/bin/quicforge-node

USER quicforge
ENV QUICFORGE_HOST=0.0.0.0 \
    QUICFORGE_PORT=8080 \
    QUICFORGE_LOG_JSON=true \
    RUST_LOG=info

EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/quicforge-node"]
# Default to the GraphQL server; override with e.g. `docker run … bench --help`.
CMD ["serve"]
