# syntax = docker/dockerfile:experimental
FROM alpine:latest AS isolate_builder
WORKDIR /usr/src/isolate
ADD isolate .
RUN apk --no-cache add libcap-dev gcc libc-dev make && \
    make isolate && \
    make install

FROM rust:alpine
COPY --from=isolate_builder /usr/local/bin/isolate /usr/local/bin/
COPY --from=isolate_builder /usr/local/bin/isolate-check-environment /usr/local/bin/
COPY --from=isolate_builder /usr/local/etc/isolate /usr/local/etc/isolate
RUN mkdir -p /var/local/lib/isolate
RUN apk --no-cache add ca-certificates libcap libc-dev gcc g++ ncurses
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    cargo install diesel_cli --no-default-features --features sqlite-bundled && \
    cargo install systemfd && \
    cargo install cargo-watch
WORKDIR /usr/src/jughisto
CMD systemfd --no-pid -s http::0.0.0.0:8000 -- cargo watch -x 'run --color always'
