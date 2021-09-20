FROM rust:1.55-alpine as builder
RUN apk --no-cache add libc-dev protoc
RUN rustup component add rustfmt
WORKDIR /usr/src/jughisto
ADD . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/usr/src/jughisto/target \
    cargo build --release && \
    cp /usr/src/jughisto/target/release/jughisto /usr/local/bin/jughisto && \
    mkdir -p /usr/local/jughisto && \
    cp -r /usr/src/jughisto/templates/ /usr/local/jughisto/templates/ && \
    cp -r /usr/src/jughisto/static/ /usr/local/jughisto/static/ && \
    cargo install diesel_cli --no-default-features --features sqlite-bundled && \
    mkdir /usr/local/jughisto/data/ && \
    diesel setup && \
    diesel migration run && \
    cp /usr/src/jughisto/data/jughisto.db /usr/local/jughisto/data/jughisto.db

FROM alpine:latest
COPY --from=builder /usr/local/bin/jughisto /usr/local/bin/
COPY --from=builder /usr/local/jughisto/templates/ /usr/local/jughisto/templates/
COPY --from=builder /usr/local/jughisto/static/ /usr/local/jughisto/static/
COPY --from=builder /usr/local/jughisto/data/jughisto.db /usr/local/jughisto/data/jughisto.db
ENV DATABASE_URL=data/jughisto.db
WORKDIR /usr/local/jughisto
CMD jughisto
