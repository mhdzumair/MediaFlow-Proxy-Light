# Build stage
FROM rust:1.84-slim-bullseye AS builder

WORKDIR /usr/src/app

RUN apt-get update && \
    apt-get install -y pkg-config libssl-dev

COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY tools ./tools

RUN cargo build --release

# Runtime stage
FROM debian:bullseye-slim

RUN apt-get update && \
    apt-get install -y ca-certificates libssl1.1 && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy the binary and config
COPY --from=builder /usr/src/app/target/release/mediaflow-proxy-light /app/
COPY config-example.toml /app/config.toml

# Create a non-root user
RUN useradd -m -U -s /bin/false mediaflow && \
    chown -R mediaflow:mediaflow /app

ENV RUST_LOG=info
ENV CONFIG_PATH=/app/config.toml

USER mediaflow

EXPOSE 8888

ENTRYPOINT ["/app/mediaflow-proxy-light"]