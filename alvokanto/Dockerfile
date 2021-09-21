FROM debian:bullseye-slim AS isolate_builder
WORKDIR /usr/src/isolate
RUN apt-get update && \
    apt-get install -yq --no-install-recommends \
        libcap-dev \
        libc-dev \
        gcc \
        make \
        grep && \
    apt-get clean && \
    rm -rf /var/lib/apt/lists/*
ADD isolate .
RUN make isolate && \
    make install

FROM rust:1.55-slim-bullseye AS builder
RUN apt-get update && \
    apt-get install -yq --no-install-recommends \
    protobuf-compiler && \
    apt-get clean && \
    rm -rf /var/lib/apt/lists/*
RUN rustup component add rustfmt
WORKDIR /usr/src/jughisto
ADD . .
WORKDIR /usr/src/jughisto/alvokanto
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/usr/src/jughisto/target \
    cargo build --release && \
    cp /usr/src/jughisto/alvokanto/target/release/alvokanto /usr/local/bin/alvokanto

FROM debian:bullseye-slim
RUN apt-get update && \
    apt-get install -yq --no-install-recommends \
        gcc \
        g++ \
        fpc \
        openjdk-17-jdk \
        python3 \
        libcap2 && \
    apt-get clean && \
    rm -rf /var/lib/apt/lists/*
COPY --from=isolate_builder /usr/local/bin/isolate /usr/local/bin/
COPY --from=isolate_builder /usr/local/bin/isolate-check-environment /usr/local/bin/
COPY --from=isolate_builder /usr/local/etc/isolate /usr/local/etc/isolate
COPY --from=builder /usr/local/bin/alvokanto /usr/local/bin/
RUN mkdir -p /var/local/lib/isolate
RUN mkdir -p /usr/local/alvokanto
WORKDIR /usr/local/alvokanto
CMD alvokanto
