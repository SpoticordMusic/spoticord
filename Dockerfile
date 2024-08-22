# This Dockerfile has been specifically crafted to be run on an AMD64 build host, where
# the build should compile for both amd64 and arm64 targets
#
# Building on any other platform, or building for only a single target will be significantly
# slower compared to a platform agnostic Dockerfile, or might not work at all
#
# This has been done to make this file be optimized for use within GitHub Actions,
# as using QEMU to compile takes way too long (multiple hours)

# Builder
FROM --platform=linux/amd64 rust:1.80.1-slim AS builder

WORKDIR /app

# Add extra build dependencies here
RUN apt-get update && apt install -yqq \
    cmake gcc-aarch64-linux-gnu binutils-aarch64-linux-gnu libpq-dev curl bzip2

# Manually compile an arm64 build of libpq
ENV PGVER=16.4
RUN curl -o postgresql.tar.bz2 https://ftp.postgresql.org/pub/source/v${PGVER}/postgresql-${PGVER}.tar.bz2 && \
    tar xjf postgresql.tar.bz2 && \
    cd postgresql-${PGVER} && \
    ./configure --host=aarch64-linux-gnu --enable-shared --disable-static --without-readline --without-zlib --without-icu && \
    cd src/interfaces/libpq && \
    make

COPY . .

RUN rustup target add x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu

# Add `--no-default-features` if you don't want stats collection
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo build --release --target=x86_64-unknown-linux-gnu && \
    RUSTFLAGS="-L /app/postgresql-${PGVER}/src/interfaces/libpq -C linker=aarch64-linux-gnu-gcc" cargo build --release --target=aarch64-unknown-linux-gnu && \
    # Copy the executables outside of /target as it'll get unmounted after this RUN command
    cp /app/target/x86_64-unknown-linux-gnu/release/spoticord /app/x86_64 && \
    cp /app/target/aarch64-unknown-linux-gnu/release/spoticord /app/aarch64

# Runtime
FROM debian:bookworm-slim

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