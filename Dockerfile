# Builder
FROM rust:1.65-buster as builder

WORKDIR /app

# Add extra build dependencies here
RUN apt-get update && apt-get install -y cmake

COPY . .

# Remove `--features stats` if you want to deploy without stats collection
RUN cargo install --path . --features stats

# Runtime
FROM debian:buster-slim

WORKDIR /app

# Add extra runtime dependencies here
RUN apt-get update && apt-get install -y openssl ca-certificates && rm -rf /var/lib/apt/lists/*

# Copy spoticord binary from builder
COPY --from=builder /usr/local/cargo/bin/spoticord ./spoticord

CMD ["./spoticord"]