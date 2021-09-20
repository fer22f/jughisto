FROM alpine:3.14 AS isolate_builder
WORKDIR /usr/src/isolate
RUN apk --no-cache add libcap-dev gcc libc-dev make grep
ADD isolate .
RUN make isolate && \
    make install

FROM rust:1.55-alpine3.14
RUN apk --no-cache add gcc g++ python3 openjdk8 protoc libcap
RUN rustup component add rustfmt
COPY --from=cmplopes/alpine-freepascal /usr/bin/fpc /usr/bin/fpc
COPY --from=cmplopes/alpine-freepascal /usr/lib/fpc /usr/lib/fpc
COPY --from=cmplopes/alpine-freepascal /usr/bin/ppcx64 /usr/bin/ppcx64
COPY --from=isolate_builder /usr/local/bin/isolate /usr/local/bin/
COPY --from=isolate_builder /usr/local/bin/isolate-check-environment /usr/local/bin/
COPY --from=isolate_builder /usr/local/etc/isolate /usr/local/etc/isolate
RUN mkdir -p /var/local/lib/isolate
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    cargo install cargo-watch
WORKDIR /usr/src/jughisto/alvokanto
CMD cargo watch -x 'run --color always'
