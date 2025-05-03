ARG RUST_VERSION

FROM rust:${RUST_VERSION}-alpine

RUN apk add --no-cache make perl musl-dev && \
    /usr/local/cargo/bin/rustup component add rust-analyzer rust-src rustfmt clippy
    
