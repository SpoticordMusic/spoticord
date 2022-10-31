# Builder
FROM rust:1.62-buster as builder

WORKDIR /app

# Add extra build dependencies here
RUN apt-get update && apt-get install -y cmake

COPY . .
RUN cargo install --path .

# Runtime
FROM debian:buster-slim

WORKDIR /app

# Add extra runtime dependencies here
RUN apt-get update && apt-get install -y openssl ca-certificates && rm -rf /var/lib/apt/lists/*

# Copy spoticord binary from builder
COPY --from=builder /usr/local/cargo/bin/spoticord ./spoticord

CMD ["./spoticord"]