# syntax=docker/dockerfile:1

ARG DEBIAN_RELEASE=trixie
ARG RUST_VERSION=1.100.0
ARG NODE_VERSION=26.13.1

FROM debian:${DEBIAN_RELEASE}-slim AS base

FROM rust:${RUST_VERSION}-slim-${DEBIAN_RELEASE} AS rust
FROM node:${NODE_VERSION}-${DEBIAN_RELEASE}-slim AS node

FROM base AS dev
ARG RUST_VERSION
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates gcc libc6-dev git \
    && rm -rf /var/lib/apt/lists/*
ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH \
    RUSTUP_TOOLCHAIN=${RUST_VERSION}
COPY --from=rust /usr/local/rustup /usr/local/rustup
COPY --from=rust /usr/local/cargo /usr/local/cargo
RUN rustup component add clippy rustfmt
COPY --from=node /usr/local/bin/node /usr/local/bin/node
COPY --from=node /usr/local/lib/node_modules /usr/local/lib/node_modules
RUN ln -s ../lib/node_modules/npm/bin/npm-cli.js /usr/local/bin/npm \
    && ln -s ../lib/node_modules/npm/bin/npx-cli.js /usr/local/bin/npx
WORKDIR /src

FROM dev AS build
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/root/.npm \
    --mount=type=cache,target=/src/target \
    test "$(node --version)" = "v$(cat web/.nvmrc)" \
        || { echo "NODE_VERSION $(node --version) differs from web/.nvmrc $(cat web/.nvmrc)" >&2; exit 1; } \
    && cargo build --release --locked \
    && install -D target/release/meshcom-status /out/meshcom-status \
    && strip --strip-all /out/meshcom-status

FROM scratch AS artifact
COPY --from=build /out/meshcom-status /meshcom-status
COPY LICENSE.md /LICENSE.md

FROM base AS prod
RUN groupadd --system --gid 10001 meshcom \
    && useradd --system --uid 10001 --gid meshcom --no-create-home --home-dir /nonexistent \
        --shell /usr/sbin/nologin meshcom
COPY --from=build /out/meshcom-status /usr/local/bin/meshcom-status
COPY LICENSE.md README.md /usr/share/doc/meshcom-status/
LABEL org.opencontainers.image.title="meshcom-status" \
      org.opencontainers.image.licenses="GPL-3.0-or-later"
ENV MESHCOM_STATUS_CONFIG=/etc/meshcom-status/meshcom-status.toml \
    MESHCOM_STATUS_WEB_LISTEN_ADDRESS=0.0.0.0:8080
USER meshcom:meshcom
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/meshcom-status"]
