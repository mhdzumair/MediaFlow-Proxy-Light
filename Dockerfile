# Build stage
FROM rust:1.84-slim-bullseye AS builder

WORKDIR /usr/src/app

# Install build dependencies (including tools needed for vendored OpenSSL)
RUN apt-get update && \
    apt-get install -y \
        pkg-config \
        libssl-dev \
        build-essential \
        make \
        perl \
        && \
    rm -rf /var/lib/apt/lists/*

# Copy dependency files first for better caching
COPY Cargo.toml Cargo.lock ./

# Create a dummy main.rs to build dependencies
RUN mkdir src && \
    echo "fn main() {}" > src/main.rs && \
    echo "pub fn add(left: usize, right: usize) -> usize { left + right }" > src/lib.rs

# Build dependencies (this layer will be cached unless Cargo.toml/Cargo.lock changes)
# Use system OpenSSL in Docker for faster builds
RUN cargo build --release && \
    rm -rf src target/release/deps/mediaflow*

# Copy source code
COPY src ./src
COPY tools ./tools

# Build the actual application
RUN cargo build --release

# Runtime stage - use distroless for smaller size and better security
FROM gcr.io/distroless/cc-debian11

WORKDIR /app

# Copy the binary
COPY --from=builder /usr/src/app/target/release/mediaflow-proxy-light /app/
COPY config-example.toml /app/config.toml

ENV RUST_LOG=info
ENV CONFIG_PATH=/app/config.toml

EXPOSE 8888

ENTRYPOINT ["/app/mediaflow-proxy-light"]