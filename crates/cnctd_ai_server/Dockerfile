# Build context: /path/to/cnctd_ai (the crate root)
# docker build -f crates/cnctd_ai_server/Dockerfile -t kebtech/cnctd-ai-server .

# -- Stage 1: Build --
FROM rust:1.88-slim AS builder

RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

WORKDIR /build

# Copy manifests first for dependency caching
COPY Cargo.toml Cargo.lock ./
COPY crates/cnctd_ai_server/Cargo.toml crates/cnctd_ai_server/Cargo.toml

# Create dummy src files so cargo can resolve dependencies
RUN mkdir -p src && echo "pub fn dummy() {}" > src/lib.rs
RUN mkdir -p crates/cnctd_ai_server/src && echo "fn main() {}" > crates/cnctd_ai_server/src/main.rs

# Build dependencies only (cached unless Cargo.toml changes)
RUN cargo build --release --manifest-path crates/cnctd_ai_server/Cargo.toml 2>/dev/null || true

# Now copy real source
COPY src/ src/
COPY crates/cnctd_ai_server/src/ crates/cnctd_ai_server/src/

# Touch files to invalidate cargo cache for our code
RUN touch src/lib.rs crates/cnctd_ai_server/src/main.rs

# Build the server
RUN cargo build --release --manifest-path crates/cnctd_ai_server/Cargo.toml

# -- Stage 2: Runtime --
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/crates/cnctd_ai_server/target/release/cnctd_ai_server /usr/local/bin/cnctd_ai_server

ENV PORT=3002

EXPOSE 3002

CMD ["cnctd_ai_server"]
