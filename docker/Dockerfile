# syntax=docker/dockerfile:1
#
# Multi-stage build for the DeTTa node and client.
# Build from the repository root:
#   docker build -f docker/Dockerfile -t detta-node:local .
# (docker-compose.yml uses context: .. so the repo root is the build context.)

FROM rust:1.88-slim-bookworm AS builder
# build-essential provides the linker (cc) used by rustc; the runtime deps are
# pure-Rust so no other system libraries are required.
RUN apt-get update \
    && apt-get install -y --no-install-recommends build-essential \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /src
COPY . .
# Cache the cargo registry and target dir across builds, then copy the two
# release binaries out before the cache mount is unmounted.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked --bin detta-node --bin detta-client \
    && mkdir -p /out \
    && cp target/release/detta-node target/release/detta-client /out/

FROM debian:bookworm-slim AS runtime
# Run as an unprivileged user. Pre-create the data/genesis mount points and own
# them so fresh named volumes inherit writable permissions on first mount.
RUN groupadd -g 10001 detta \
    && useradd -r -u 10001 -g detta detta \
    && mkdir -p /data /genesis \
    && chown -R detta:detta /data /genesis
COPY --from=builder /out/detta-node /out/detta-client /usr/local/bin/
COPY docker/entrypoint.sh /usr/local/bin/entrypoint.sh
RUN chmod +x /usr/local/bin/entrypoint.sh
USER detta
WORKDIR /data
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/entrypoint.sh"]
