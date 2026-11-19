# syntax=docker/dockerfile:1
FROM rust:1.99.0-slim-bookworm AS build
RUN apt-get update && apt-get install -y --no-install-recommends cmake build-essential && rm -rf /var/lib/apt/lists/*
WORKDIR /src
ARG CARGO_JOBS=1
# Client WASM (ADR 0014) : cible et wasm-bindgen à la version exacte du crate verrouillé.
ARG WASM_BINDGEN_VERSION=0.2.130
RUN rustup target add wasm32-unknown-unknown && \
    cargo install --locked wasm-bindgen-cli --version ${WASM_BINDGEN_VERSION} -j ${CARGO_JOBS}

# Mise en cache des dépendances du workspace unifié
COPY Cargo.toml Cargo.lock ./
COPY frontend/Cargo.toml ./frontend/
COPY shared/Cargo.toml ./shared/
COPY client/Cargo.toml ./client/
RUN mkdir -p src frontend/src shared/src client/src && \
    echo '' > shared/src/lib.rs && \
    echo '' > client/src/lib.rs && \
    echo 'fn main() {}' > src/main.rs && \
    echo '' > frontend/src/lib.rs && \
    echo 'fn main() {}' > frontend/src/main.rs

RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/src/target \
    cargo build --locked --release -j ${CARGO_JOBS}

# Compilation des sources et génération des binaires et de l'interface
COPY .sqlx ./.sqlx
COPY src ./src
COPY migrations ./migrations
COPY docs/openapi.yaml ./docs/openapi.yaml
COPY frontend/src ./frontend/src
COPY frontend/dist/forgearr.svg ./frontend/dist/
COPY shared/src ./shared/src
COPY client/src ./client/src
ENV SQLX_OFFLINE=true

RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/src/target \
    find src frontend/src shared/src client/src -type f -exec touch {} + && \
    cargo build --locked --release -j ${CARGO_JOBS} && \
    cp target/release/forgearr /usr/local/bin/forgearr && \
    cargo run --locked --release -p forgearr-frontend -j ${CARGO_JOBS} && \
    test -s /src/frontend/dist/index.html && \
    cargo build --locked --release -p forgearr-client --target wasm32-unknown-unknown -j ${CARGO_JOBS} && \
    wasm-bindgen --target web --no-typescript --out-name flux_client --out-dir /src/frontend/dist/pkg \
      target/wasm32-unknown-unknown/release/forgearr_client.wasm && \
    test -s /src/frontend/dist/pkg/flux_client_bg.wasm

FROM debian:bookworm-slim@sha256:734a72b3d6142a71993b4bc580ca01d8ca687f77a2c422d96a961d3376b179bc AS runtime
ARG APP_VERSION=0.1.0
ARG VCS_REF=local
LABEL org.opencontainers.image.title="Forgearr" org.opencontainers.image.version=$APP_VERSION org.opencontainers.image.revision=$VCS_REF
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 app \
    && mkdir -p /config /data/input /data/output /data/staging /work /logs /usr/share/forgearr/frontend \
    && chown -R app:app /config /data /work /logs /usr/share/forgearr/frontend
COPY --from=build /usr/local/bin/forgearr /usr/local/bin/forgearr
RUN ln -s forgearr /usr/local/bin/flux-de-fichiers
COPY --from=build /src/frontend/dist/ /usr/share/forgearr/frontend/
ENV CONFIG_DIR=/config DATA_DIR=/data INPUT_DIR=/data/input WORK_DIR=/work LOG_DIR=/logs STATIC_DIR=/usr/share/forgearr/frontend
VOLUME ["/config", "/data", "/work", "/logs"]
USER 10001
EXPOSE 3000
HEALTHCHECK --interval=10s --timeout=3s --retries=3 CMD grep -qa worker /proc/1/cmdline || curl -fsS http://localhost:3000/health || exit 1
ENTRYPOINT ["forgearr"]
CMD ["central"]
