FROM rust:1.55-slim-bullseye
RUN apt-get update && \
    apt-get install -yq --no-install-recommends \
        libc-dev \
        protobuf-compiler \
        gcc \
        g++ \
        libpq-dev && \
    apt-get clean && \
    rm -rf /var/lib/apt/lists/*
RUN rustup component add rustfmt
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    cargo install diesel_cli --no-default-features --features postgres && \
    cargo install systemfd && \
    cargo install cargo-watch
WORKDIR /usr/src/jughisto
CMD diesel setup && diesel migration run && systemfd --no-pid -s http::0.0.0.0:8000 -- cargo watch -x 'run --color always' -i alvokanto/
