# syntax=docker/dockerfile:1
# Hopwire engine image. Build from the repo root:
#   docker build -f deploy/engine.Dockerfile -t hopwire/engine .
#
# Ships two binaries:
#   /usr/local/bin/hopwire-server  (entrypoint: HTTP API + Postgres)
#   /usr/local/bin/hopwire-bench   (pilot replay tool, run via --entrypoint)
#
# No HEALTHCHECK: distroless has no shell/curl. Probe GET /healthz from outside
# (compose checks it through the console's nginx proxy; Helm uses httpGet probes).

FROM rust:1-bookworm AS chef
RUN cargo install cargo-chef --locked
WORKDIR /src

# 1. Dependency recipe (changes only when Cargo.toml/Cargo.lock change).
FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# 2. Build deps (cached layer), then the workspace binaries.
FROM chef AS builder
# sqlx compile-time macros (if used) read the committed .sqlx/ cache, never a live DB.
ENV SQLX_OFFLINE=true
COPY --from=planner /src/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
COPY . .
RUN cargo build --release --locked -p hopwire-server -p hopwire-bench

# 3. Runtime: glibc + libgcc + CA certs, no shell, runs as uid 65532.
FROM gcr.io/distroless/cc-debian12:nonroot
COPY --from=builder /src/target/release/hopwire-server /usr/local/bin/hopwire-server
COPY --from=builder /src/target/release/hopwire-bench /usr/local/bin/hopwire-bench
COPY config.example.toml /etc/hopwire/config.toml
ENV HOPWIRE_CONFIG=/etc/hopwire/config.toml \
    HOPWIRE_BIND=0.0.0.0:8080
EXPOSE 8080
USER nonroot:nonroot
ENTRYPOINT ["/usr/local/bin/hopwire-server"]
