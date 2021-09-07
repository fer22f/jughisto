FROM alpine:latest AS isolate_builder
WORKDIR /usr/src/isolate
ADD isolate .
RUN apk --no-cache add libcap-dev gcc libc-dev make grep && \
    make isolate && \
    make install

FROM rust:alpine as builder
RUN apk --no-cache add libc-dev
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
RUN apk --no-cache add ca-certificates libcap gcc g++ python3 openjdk8 libc-dev ncurses
COPY --from=cmplopes/alpine-freepascal /usr/bin/fpc /usr/bin/fpc
COPY --from=cmplopes/alpine-freepascal /usr/lib/fpc /usr/lib/fpc
COPY --from=cmplopes/alpine-freepascal /usr/bin/ppcx64 /usr/bin/ppcx64
COPY --from=builder /usr/local/bin/jughisto /usr/local/bin/
COPY --from=builder /usr/local/jughisto/templates/ /usr/local/jughisto/templates/
COPY --from=builder /usr/local/jughisto/static/ /usr/local/jughisto/static/
COPY --from=builder /usr/local/jughisto/data/jughisto.db /usr/local/jughisto/data/jughisto.db
COPY --from=isolate_builder /usr/local/bin/isolate /usr/local/bin/
COPY --from=isolate_builder /usr/local/bin/isolate-check-environment /usr/local/bin/
COPY --from=isolate_builder /usr/local/etc/isolate /usr/local/etc/isolate
RUN mkdir -p /var/local/lib/isolate
ENV DATABASE_URL=data/jughisto.db
WORKDIR /usr/local/jughisto
CMD jughisto
