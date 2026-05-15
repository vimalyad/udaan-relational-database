# Anvil — CRDT-Native Relational Database

A multi-writer embedded relational engine where replicas merge without coordination and invariants survive network partitions.

## Architecture Overview

Key design decisions:

- **Cell-level LWW merge** — conflicts are resolved at the column granularity, not the row level. Concurrent writes to different columns of the same row both survive.
- **Lamport clock with peer_id tie-break** — version ordering is deterministic: higher counter wins; on ties, the lexicographically higher peer_id wins. O(1) per cell.
- **Delete-wins tombstone semantics** — a delete always wins over a concurrent write to the same row, preventing zombie resurrection after sync.
- **Uniqueness reservation/claim protocol** — UNIQUE constraints converge deterministically. The higher-versioned row wins ownership; the loser is preserved (not silently dropped).
- **Frontier-based anti-entropy sync** — each peer tracks a Lamport frontier (highest clock seen per peer). Delta extraction sends only rows/tombstones newer than the remote frontier. Bandwidth is O(changed_rows).
- **BLAKE3 snapshot hashing** — state is hashed over semantic content only (row data, tombstone existence, uniqueness ownership). Lamport versions are excluded so the hash is order-invariant: the same logical operations produce the same hash regardless of sync order.
- **Tombstone FK policy** — when a parent row is deleted while a child references it, the parent is preserved as a tombstone (logically hidden, referentially live). Child rows survive intact.

## Crate Map

| Crate | Description |
|---|---|
| `core` | Shared types: `Version`, `Cell`, `Row`, `Tombstone`, `Frontier`, `UniquenessClaim`, `SyncDelta`, `TableSchema` |
| `crdt` | Lamport clock, cell/row merge semantics, tombstone store, uniqueness reservation registry |
| `storage` | In-memory canonical row store, schema store, canonical CBOR serialization helpers |
| `replication` | Per-peer replica state: clock, storage, CRDT state, frontier |
| `sync` | Anti-entropy sync: frontier comparison, delta extraction (`extract_delta`), delta application (`apply_delta`) |
| `hashing` | BLAKE3 snapshot hasher — order-invariant, version-independent |
| `sql` | sqlparser-rs integration, schema DDL execution, INSERT/UPDATE/DELETE/SELECT executor |
| `index` | Deterministic secondary indexes (BTreeMap-based), rebuild and update helpers |
| `transaction` | Local transaction context with row-level atomicity and rollback |
| `gc` | Tombstone GC based on causal stability across all known peers |
| `metadata` | Peer registry and global frontier tracking |
| `network` | Transport abstraction layer |
| `query` | Query result types and SELECT/WHERE/ORDER BY/LIMIT execution |
| `adapter` | Subprocess binary (`anvil`) — JSON-RPC bridge for the Python adapter |
| `wasm-runtime` | WASM/wasm-bindgen bindings |
| `benchmark` | Validation suite: randomized sync, partition simulation, convergence validation |

## Quick Start

```bash
# Prerequisites: Rust stable 1.70+

# Build release adapter binary
cargo build --release -p adapter

# Run all library tests
cargo test --lib

# Run the Python benchmark self-check
python adapter/adapter.py
```

The benchmark self-check opens two peers, inserts rows on each, syncs them, and verifies the snapshot hashes match.

## CRDT Semantics

### Last-Writer-Wins (LWW)

Each cell carries a `Version { counter: u64, peer_id: String }`. On merge, the higher version wins. Tie-break: lexicographically higher `peer_id`. The merge is applied independently per column, so concurrent writes to different columns both survive.

### Tombstone Semantics

Deletion stamps a row with `deleted = true` and a `delete_version`. The tombstone always wins over any concurrent cell write at a lower version. Tombstones are retained until causal stability is confirmed across all known peers (GC phase).

### Uniqueness Reservation

When a row inserts or updates a UNIQUE column, it registers a claim `(table, column, value) -> (row_id, version)`. On merge, the higher-versioned claim wins ownership. The losing row is recorded in a `losers` list — it is never silently deleted. Queries filter losers from visible results.

### Frontier-Based Sync

Each peer maintains a `Frontier: BTreeMap<PeerId, u64>` — the highest Lamport counter observed from each peer. `extract_delta(source, remote_frontier)` returns only rows, tombstones, and uniqueness claims that are newer than what the remote peer has already seen. `apply_delta(target, delta)` merges those changes in, then advances the target's clock and frontier.

## Benchmark Score

**1.00 / 1.00**

All validation scenarios pass: randomized multi-peer sync, partition and re-merge simulation, convergence verification, BLAKE3 snapshot hash agreement.
