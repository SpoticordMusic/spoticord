# Builder
FROM --platform=linux/amd64 rust:1.72.1-buster as builder

WORKDIR /app

# Add extra build dependencies here
RUN apt-get update && apt-get install -yqq \
    cmake gcc-aarch64-linux-gnu  binutils-aarch64-linux-gnu 

COPY . .

RUN rustup target add x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu 

# Remove `--features=stats` if you want to deploy without stats collection
#RUN cargo build --features=stats --release \
#    --target=x86_64-unknown-linux-gnu --target=aarch64-unknown-linux-gnu

RUN echo woah

# Runtime
FROM debian:buster-slim

ARG TARGETPLATFORM
ENV TARGETPLATFORM=$TARGETPLATFORM

# Add extra runtime dependencies here
# RUN apt-get update && apt-get install -yqq --no-install-recommends \
#    openssl ca-certificates && rm -rf /var/lib/apt/lists/*

# Copy spoticord binaries from builder to /tmp
#COPY --from=builder \
#    /app/target/x86_64-unknown-linux-gnu/release/spoticord /tmp/x86_64
#COPY --from=builder \
#    /app/target/aarch64-unknown-linux-gnu/release/spoticord /tmp/aarch64

# Copy appropiate binary for target arch from /tmp  
#RUN if [ "$TARGETPLATFORM" = "linux/amd64" ]; then \
#        cp /tmp/x86_64 /usr/local/bin/spoticord; \
#    elif [ "$TARGETPLATFORM" = "linux/arm64" ]; then \
#        cp /tmp/aarch64 /usr/local/bin/spoticord; \
#    fi

# Delete unused binaries
# RUN rm -rvf /tmp/x86_64 /tmp/aarch64

ENTRYPOINT [ "/usr/local/bin/spoticord" ]
