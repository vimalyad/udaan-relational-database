# Anvil - reproducible benchmark container.
#
# Build: docker build -t anvil .
# Run:   docker run --rm anvil

FROM rust:1.95-slim-bookworm AS builder

WORKDIR /app

RUN apt-get update && apt-get install -y --no-install-recommends \
    python3 \
    && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates/ crates/

RUN cargo build --release -p adapter

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    python3 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

RUN mkdir -p target/release
COPY --from=builder /app/target/release/anvil ./target/release/anvil
COPY bench-harness/ ./bench-harness/
COPY adapter/ ./adapter/

WORKDIR /app/bench-harness/bench-p01-crdt

CMD ["python3", "run.py", "--adapter", "adapters.anvil:Engine", "--fk-policy", "tombstone", "--out", "-"]
