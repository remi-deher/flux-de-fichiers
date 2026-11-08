# syntax=docker/dockerfile:1
FROM rust:1.99.0-slim-bookworm AS build
RUN apt-get update && apt-get install -y --no-install-recommends cmake build-essential && rm -rf /var/lib/apt/lists/*
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo 'fn main() {}' > src/main.rs
RUN --mount=type=cache,target=/usr/local/cargo/registry --mount=type=cache,target=/src/target cargo build --locked --release -j 1
COPY src ./src
COPY migrations ./migrations
RUN --mount=type=cache,target=/usr/local/cargo/registry --mount=type=cache,target=/src/target touch src/main.rs && cargo build --locked --release -j 1 && cp target/release/flux-de-fichiers /usr/local/bin/flux-de-fichiers

FROM debian:bookworm-slim@sha256:734a72b3d6142a71993b4bc580ca01d8ca687f77a2c422d96a961d3376b179bc AS runtime
ARG APP_VERSION=0.1.0
ARG VCS_REF=local
LABEL org.opencontainers.image.title="Flux de Fichiers" org.opencontainers.image.version=$APP_VERSION org.opencontainers.image.revision=$VCS_REF
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates mkvtoolnix curl && rm -rf /var/lib/apt/lists/* && useradd --system --uid 10001 app && mkdir /data /work /logs && chown app /data /work /logs
COPY --from=build /usr/local/bin/flux-de-fichiers /usr/local/bin/flux-de-fichiers
ENV DATA_DIR=/data WORK_DIR=/work LOG_DIR=/logs
USER 10001
EXPOSE 3443
ENTRYPOINT ["flux-de-fichiers"]
CMD ["coordinator"]
