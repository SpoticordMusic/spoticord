# Builder
FROM --platform=linux/amd64 rust:1.80.1-slim AS builder

WORKDIR /app

# Add extra build dependencies here
RUN apt-get update && apt-get install -yqq \
    cmake gcc-aarch64-linux-gnu binutils-aarch64-linux-gnu

COPY . .

RUN rustup target add x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu

# Add `--no-default-features` if you don't want stats collection
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo build --release --target=x86_64-unknown-linux-gnu --target=aarch64-unknown-linux-gnu && \
    # Copy the executables outside of /target as it'll get unmounted after this RUN command
    cp /app/target/x86_64-unknown-linux-gnu/release/spoticord /app/x86_64 && \
    cp /app/target/aarch64-unknown-linux-gnu/release/spoticord /app/aarch64

# Runtime
FROM debian:buster-slim

ARG TARGETPLATFORM
ENV TARGETPLATFORM=${TARGETPLATFORM}

# Add extra runtime dependencies here
RUN apt update && apt install -y ca-certificates

# Copy spoticord binaries from builder to /tmp so we can dynamically use them
COPY --from=builder \
    /app/x86_64 /tmp/x86_64
COPY --from=builder \
    /app/aarch64 /tmp/aarch64

# Copy appropriate binary for target arch from /tmp
RUN if [ "${TARGETPLATFORM}" = "linux/amd64" ]; then \
    cp /tmp/x86_64 /usr/local/bin/spoticord; \
    elif [ "${TARGETPLATFORM}" = "linux/arm64" ]; then \
    cp /tmp/aarch64 /usr/local/bin/spoticord; \
    fi

# Delete unused binaries
RUN rm -rvf /tmp/x86_64 /tmp/aarch64

ENTRYPOINT [ "/usr/local/bin/spoticord" ]