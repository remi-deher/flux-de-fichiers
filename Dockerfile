# syntax=docker/dockerfile:1
FROM rust:1.99.0-slim-bookworm AS frontend-build
WORKDIR /src/frontend
ARG CARGO_JOBS=1
COPY frontend/Cargo.toml frontend/Cargo.lock ./
RUN mkdir src && echo 'pub fn render_full_page() -> String { String::new() }' > src/lib.rs && echo 'fn main() {}' > src/main.rs
RUN --mount=type=cache,target=/usr/local/cargo/registry --mount=type=cache,target=/src/frontend/target \
    cargo build --locked --release -j ${CARGO_JOBS}
COPY frontend/src ./src
COPY frontend/static ./static
RUN --mount=type=cache,target=/usr/local/cargo/registry --mount=type=cache,target=/src/frontend/target \
    find src static -type f -exec touch {} + && cargo run --locked --release -j ${CARGO_JOBS} && test -s /src/frontend/dist/index.html

FROM rust:1.99.0-slim-bookworm AS build
RUN apt-get update && apt-get install -y --no-install-recommends cmake build-essential && rm -rf /var/lib/apt/lists/*
WORKDIR /src
ARG CARGO_JOBS=1
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo 'fn main() {}' > src/main.rs
RUN --mount=type=cache,target=/usr/local/cargo/registry --mount=type=cache,target=/src/target cargo build --locked --release -j ${CARGO_JOBS}
COPY src ./src
COPY migrations ./migrations
RUN --mount=type=cache,target=/usr/local/cargo/registry --mount=type=cache,target=/src/target find src -type f -exec touch {} + && cargo build --locked --release -j ${CARGO_JOBS} && cp target/release/flux-de-fichiers /usr/local/bin/flux-de-fichiers

FROM debian:bookworm-slim@sha256:734a72b3d6142a71993b4bc580ca01d8ca687f77a2c422d96a961d3376b179bc AS runtime
ARG APP_VERSION=0.1.0
ARG VCS_REF=local
LABEL org.opencontainers.image.title="Flux de Fichiers" org.opencontainers.image.version=$APP_VERSION org.opencontainers.image.revision=$VCS_REF
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates mkvtoolnix curl mediainfo && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 app \
    && mkdir -p /config /data /work /logs /usr/share/flux-de-fichiers/frontend \
    && chown -R app:app /config /data /work /logs /usr/share/flux-de-fichiers/frontend
COPY --from=build /usr/local/bin/flux-de-fichiers /usr/local/bin/flux-de-fichiers
COPY --from=frontend-build /src/frontend/dist/ /usr/share/flux-de-fichiers/frontend/
ENV CONFIG_DIR=/config DATA_DIR=/data WORK_DIR=/work LOG_DIR=/logs STATIC_DIR=/usr/share/flux-de-fichiers/frontend
VOLUME ["/config", "/data", "/work", "/logs"]
USER 10001
EXPOSE 3000
HEALTHCHECK --interval=10s --timeout=3s --retries=3 CMD grep -qa worker /proc/1/cmdline || curl -fsS http://localhost:3000/health || exit 1
ENTRYPOINT ["flux-de-fichiers"]
CMD ["coordinator"]
