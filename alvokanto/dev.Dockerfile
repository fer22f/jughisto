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

FROM rust:1.55-slim-bullseye
RUN apt-get update && \
    apt-get install -yq --no-install-recommends \
        protobuf-compiler \
        gcc \
        g++ \
        fpc \
        openjdk-17-jdk \
        python3 \
        libcap2 && \
    apt-get clean && \
    rm -rf /var/lib/apt/lists/*
RUN rustup component add rustfmt
COPY --from=isolate_builder /usr/local/bin/isolate /usr/local/bin/
COPY --from=isolate_builder /usr/local/bin/isolate-check-environment /usr/local/bin/
COPY --from=isolate_builder /usr/local/etc/isolate /usr/local/etc/isolate
RUN mkdir -p /var/local/lib/isolate
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    cargo install cargo-watch
WORKDIR /usr/src/jughisto/alvokanto
CMD cargo watch -x 'run --color always'
