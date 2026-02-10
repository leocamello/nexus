# Build stage
FROM rust:latest AS builder

WORKDIR /app

# Copy manifests and compile-time assets
COPY Cargo.toml Cargo.lock nexus.example.toml ./

# Create dummy src to build dependencies
RUN mkdir src && \
    echo "fn main() {}" > src/main.rs && \
    echo "pub fn dummy() {}" > src/lib.rs

# Build dependencies only (cached layer)
RUN cargo build --release && \
    rm -rf src target/release/deps/nexus* target/release/deps/libnexus*

# Copy actual source code
COPY src ./src

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
