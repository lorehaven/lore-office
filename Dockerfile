# lore-office: the launcher, docs, sheets, notes and pdf, and the cloud, as one image (one service, one port).
FROM rust:1.94-alpine AS builder

RUN apk update && apk add --no-cache build-base openssl openssl-dev openssl-libs-static musl-dev bash libc-dev
RUN rustup target add wasm32-unknown-unknown

# The quench-* crates come from the `ennor` registry (the same build args `anvil docker build` passes everywhere).
ARG CARGO_REGISTRIES_ENNOR_INDEX=sparse+https://ennor.ddns.net/index/
ARG CARGO_REGISTRIES_ENNOR_TOKEN=
RUN mkdir -p /usr/local/cargo && \
    printf "[registries.ennor]\nindex = \"%s\"\n" "${CARGO_REGISTRIES_ENNOR_INDEX}" > /usr/local/cargo/config.toml && \
    if [ -n "${CARGO_REGISTRIES_ENNOR_TOKEN}" ]; then \
      printf "token = \"%s\"\n" "${CARGO_REGISTRIES_ENNOR_TOKEN}" >> /usr/local/cargo/config.toml; \
    fi

WORKDIR /work
COPY . .

# The pages' modules, then the server.
RUN bash scripts/web.sh
RUN cargo build --release -p lore-office-server

FROM alpine:3.22

RUN apk add --no-cache ca-certificates && addgroup -g 10001 lore && adduser -D -u 10001 -G lore lore
WORKDIR /app
COPY --from=builder --chown=10001:10001 /work/target/release/lore-office-server /app/lore-office-server
COPY --from=builder --chown=10001:10001 /work/web /app/web
# The person's documents when the storage is local (a volume belongs here).
RUN mkdir -p /data && chown 10001:10001 /data
USER 10001:10001

ENV RUST_LOG="info" HOST="0.0.0.0" PORT="8080" WASM_DIR="/app/web" DATA_DIR="/data" CLOUD_STORAGE="local"
EXPOSE 8080
CMD ["/app/lore-office-server"]
