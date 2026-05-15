# Anvil — CRDT-Native Relational Engine
# Reproducible build: compiles the Rust engine and runs the P-01 benchmark.
#
# Build:  docker build -t anvil .
# Verify: docker run --rm anvil
# Score:  docker run --rm anvil python3 self_check.py \
#           --adapter adapters.anvil:Engine --fk-policy tombstone

FROM rust:1.95-slim-bookworm AS builder

WORKDIR /app

RUN apt-get update && apt-get install -y --no-install-recommends \
    python3 \
    && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates/ crates/

RUN cargo build --release -p adapter

# ── Runtime image ────────────────────────────────────────────────────────────
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    python3 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

RUN mkdir -p target/release
COPY --from=builder /app/target/release/anvil ./target/release/anvil
COPY bench-harness/ ./bench-harness/
COPY adapter/adapter.py ./adapter/adapter.py

WORKDIR /app/bench-harness/bench-p01-crdt

CMD ["python3", "self_check.py", "--adapter", "adapters.anvil:Engine", "--fk-policy", "tombstone"]
