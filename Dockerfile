# FGP GitHub Daemon Docker Image
#
# Provides fast GitHub operations via GraphQL and REST API.
# Uses multi-stage build for minimal image size.

# Stage 1: Build the Rust binary
FROM rust:1.75-slim-bookworm AS builder

WORKDIR /app

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy manifests first for better layer caching
COPY Cargo.toml Cargo.lock ./

# Create dummy src to build dependencies
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release && rm -rf src target/release/fgp-github

# Copy actual source and build
COPY src ./src
RUN touch src/main.rs && cargo build --release

# Stage 2: Minimal runtime image
FROM debian:bookworm-slim

# Install only CA certificates for HTTPS
RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user for security
RUN useradd -m -s /bin/bash fgp

# Copy binary from builder
COPY --from=builder /app/target/release/fgp-github /usr/local/bin/

# Set up FGP directory structure
RUN mkdir -p /home/fgp/.fgp/services/github/logs \
    && chown -R fgp:fgp /home/fgp/.fgp

USER fgp
WORKDIR /home/fgp

ENV FGP_SOCKET_DIR=/home/fgp/.fgp/services

# Health check
HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
    CMD fgp-github health || exit 1

# Mount point for socket (token passed via env var GITHUB_TOKEN)
VOLUME ["/home/fgp/.fgp/services"]

ENTRYPOINT ["fgp-github"]
CMD ["start", "--foreground"]
