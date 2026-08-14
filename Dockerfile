# Reference container for turbo-picard evaluation and nf-core side-by-side runs.
FROM rust:1.89-bookworm AS builder
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        clang \
        cmake \
        libbz2-dev \
        libclang-dev \
        libcurl4-openssl-dev \
        liblzma-dev \
        libssl-dev \
        make \
        pkg-config \
        zlib1g-dev \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /src
COPY . .
RUN cargo build --release --locked -p turbo-picard-cli --bin turbo-picard --bin picard

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /src/target/release/turbo-picard /usr/local/bin/turbo-picard
COPY --from=builder /src/target/release/picard /usr/local/bin/picard
ENV TURBO_PICARD_THREADS=4
ENTRYPOINT ["turbo-picard"]
