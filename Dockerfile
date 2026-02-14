# Build stage
FROM rust:latest AS builder

WORKDIR /app

# Copy manifests and compile-time assets
COPY Cargo.toml Cargo.lock nexus.example.toml ./

# Create dummy src and bench stubs to satisfy Cargo.toml manifest
RUN mkdir src && \
    echo "fn main() {}" > src/main.rs && \
    echo "pub fn dummy() {}" > src/lib.rs && \
    mkdir -p benches && \
    echo "fn main() {}" > benches/cli_startup.rs && \
    echo "fn main() {}" > benches/config_parsing.rs && \
    echo "fn main() {}" > benches/metrics.rs && \
    echo "fn main() {}" > benches/routing.rs && \
    mkdir -p dashboard

# Build dependencies only (cached layer)
RUN cargo build --release && \
    rm -rf src target/release/deps/nexus* target/release/deps/libnexus*

# Copy actual source code and embedded assets
COPY src ./src
COPY dashboard ./dashboard

# Build the actual binary
RUN cargo build --release

# Runtime stage - minimal image
FROM debian:bookworm-slim AS runtime

# Install runtime dependencies
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
        ca-certificates \
        libssl3 && \
    rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd --create-home --shell /bin/bash nexus

WORKDIR /home/nexus

# Copy binary from builder (example config is embedded via include_str!)
COPY --from=builder /app/target/release/nexus /usr/local/bin/nexus

# Switch to non-root user
USER nexus

# Default port
EXPOSE 3000

# Health check
HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
    CMD ["/usr/local/bin/nexus", "health", "--format", "json"]

# Default command
ENTRYPOINT ["/usr/local/bin/nexus"]
CMD ["serve"]
