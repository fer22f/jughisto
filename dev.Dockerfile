FROM rust:1.55-alpine3.14
RUN apk --no-cache add libc-dev protoc gcc g++
RUN rustup component add rustfmt
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    cargo install diesel_cli --no-default-features --features sqlite-bundled && \
    cargo install systemfd && \
    cargo install cargo-watch
WORKDIR /usr/src/jughisto
CMD systemfd --no-pid -s http::0.0.0.0:8000 -- cargo watch -x 'run --color always' -i alvokanto/
