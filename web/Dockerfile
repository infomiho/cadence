# Build context is the repository root so the site can embed the shared
# assets/ artwork. Coolify: Base Directory "/", Dockerfile "/web/Dockerfile".

FROM rust:1.97-slim-bookworm AS builder

RUN apt-get update \
    && apt-get install -y --no-install-recommends build-essential cmake pkg-config \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY web/Cargo.toml web/Cargo.lock ./web/

# Compile dependencies against a stub so source edits do not rebuild them.
RUN mkdir -p web/src \
    && echo 'fn main() {}' > web/src/main.rs \
    && cargo build --release --locked --manifest-path web/Cargo.toml

COPY web/src ./web/src
COPY web/static ./web/static
COPY assets/cadence-mark.svg assets/cadence.webp ./assets/

RUN cargo build --release --locked --manifest-path web/Cargo.toml

FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/web/target/release/cadence-web /usr/local/bin/cadence-web

ENV PORT=3000
EXPOSE 3000

CMD ["cadence-web"]
