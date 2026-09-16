# Build context is the web/ directory. Coolify: Base Directory "/web",
# Dockerfile Location "/Dockerfile".

FROM rust:1.97-slim-bookworm AS builder

RUN apt-get update \
    && apt-get install -y --no-install-recommends build-essential cmake pkg-config \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY Cargo.toml Cargo.lock ./

# Compile dependencies against a stub so source edits do not rebuild them.
RUN mkdir -p src \
    && echo 'fn main() {}' > src/main.rs \
    && cargo build --release --locked

COPY src ./src
COPY static ./static

# Docker resets copied file times, which can look older than the stub artifact.
# Touch the sources so Cargo rebuilds the crate instead of reusing the stub.
RUN find src -name '*.rs' -exec touch {} + \
    && cargo build --release --locked

FROM debian:bookworm-slim

# Coolify's HTTP health check runs curl inside the container.
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/cadence-web /usr/local/bin/cadence-web

ENV PORT=3000
EXPOSE 3000

CMD ["cadence-web"]
