# Anvil — CRDT-Native Relational Database

> A relational engine that never disagrees, even when nobody is online.

Anvil is a multi-writer embedded OLTP engine with a SQLite-like SQL surface and CRDT-native internals. Multiple replicas mutate locally without coordination. Merges converge. Relational invariants — uniqueness, foreign keys, secondary indexes — survive arbitrary network partitions.

**Benchmark score: 1.00 / 1.00** across convergence, uniqueness, FK, cell-level merge, order-invariance, and randomized multi-peer scenarios.

---

## Quick Start

**Prerequisites:** Rust 1.95+, Python 3.10+

```bash
git clone https://github.com/vimalyad/udaan-relational-database
cd udaan-relational-database

# Build the engine binary
cargo build --release -p adapter

# Run the benchmark self-check (all axes must pass)
cd bench-harness/bench-p01-crdt
python3 self_check.py --adapter adapters.anvil:Engine --fk-policy tombstone
```

Expected output:
```
  AXIS                          PASS    WEIGHT
  convergence                     PASS    0.30
  uniqueness:users.email          PASS    0.20
  fk                              PASS    0.15
  cell-level:u1                   PASS    0.10
  order-invariance                PASS    0.10
  randomized                      PASS    0.15
  WEIGHTED SCORE                1.00  / 1.00
```

### Using Docker (reproducible, no Rust required)

```bash
docker build -t anvil .
docker run --rm anvil
```

---

## What It Does

| Capability | Implementation |
|---|---|
| `CREATE TABLE / INDEX` | Full DDL with PK, UNIQUE, NOT NULL, DEFAULT, REFERENCES |
| `INSERT / UPDATE / DELETE` | Cell-level CRDT writes with Lamport versioning |
| `SELECT … WHERE … ORDER BY … LIMIT` | Operates on deterministically merged local replica |
| Concurrent writes, no coordinator | Peers write offline; sync at any time in any order |
| Uniqueness under partition | Reservation/claim protocol — loser preserved, not dropped |
| Foreign keys under partition | Tombstone policy — child survives parent deletion |
| Convergence verification | BLAKE3 snapshot hash — order-invariant, version-independent |

---

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│                    Python / Benchmark                    │
│                     adapter.py (RPC)                     │
└───────────────────────┬─────────────────────────────────┘
                        │  JSON-RPC over stdin/stdout
┌───────────────────────▼─────────────────────────────────┐
│                  anvil (Rust binary)                     │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌────────┐  │
│  │   SQL    │  │   Sync   │  │ Hashing  │  │  GC    │  │
│  │ Executor │  │  Engine  │  │ (BLAKE3) │  │        │  │
│  └────┬─────┘  └────┬─────┘  └──────────┘  └────────┘  │
│       │              │                                   │
│  ┌────▼──────────────▼──────────────────────────────┐   │
│  │              ReplicaState (per peer)              │   │
│  │  storage · schemas · tombstones · uniqueness      │   │
│  │  frontier · clock · indexes                       │   │
│  └───────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────┘
```

### Key Design Decisions

| Decision | Choice | Reason |
|---|---|---|
| Conflict resolution | Cell-level LWW | Row-level LWW loses concurrent updates to different columns |
| Clock | Lamport scalar `(counter, peer_id)` | O(1) per cell; O(writers) total — bounded metadata |
| Delete semantics | Delete-wins tombstone | Prevents zombie row resurrection after partition |
| Uniqueness | Reservation/claim protocol | Pure CRDTs cannot enforce uniqueness; 2PC blocks under partition |
| Foreign keys | Tombstone policy | Child survives parent deletion; most information-preserving |
| Hash | BLAKE3 over semantic content only | Excluding clock metadata makes hash order-invariant |

---

## Crate Map

| Crate | Responsibility |
|---|---|
| `core` | Shared types: `Version`, `Cell`, `Row`, `Tombstone`, `Frontier`, `UniquenessClaim`, `SyncDelta`, `TableSchema` |
| `crdt` | Lamport clock, cell/row/table merge, tombstone store, uniqueness registry |
| `storage` | In-memory BTreeMap row store, schema store, CBOR serialization |
| `replication` | Per-peer replica: clock, storage, CRDT state, frontier |
| `sync` | `extract_delta`, `apply_delta`, `sync_peers`, `sync_to_quiescence` |
| `hashing` | BLAKE3 snapshot hasher — order-invariant, version-excluded |
| `sql` | sqlparser-rs DDL/DML executor with CRDT write semantics |
| `index` | Deterministic BTreeMap secondary indexes |
| `transaction` | Local transaction context with row-level atomicity and rollback |
| `gc` | Causal-stability tombstone garbage collection |
| `query` | Query result types, SELECT/WHERE/ORDER BY/LIMIT |
| `network` | Transport abstraction layer |
| `wasm-runtime` | WASM/wasm-bindgen bindings |
| `adapter` | `anvil` subprocess binary — JSON-RPC bridge |
| `benchmark` | Validation suite: randomized sync, partition simulation, convergence |

---

## Running Tests

```bash
# Library unit tests (all crates)
cargo test --lib --all

# Benchmark integration tests (randomized sync, partition, convergence)
cargo test -p benchmark

# Full benchmark with custom seeds (stress test)
cd bench-harness/bench-p01-crdt
python3 run.py --adapter adapters.anvil:Engine --fk-policy tombstone \
  --randomized-seeds 9999 31415 27182 16180 11235 \
  --rand-peers 5 --rand-ops 150 --out report.json
```

---

## Dependencies

| Dependency | Version | Purpose |
|---|---|---|
| `sqlparser` | 0.54 | SQL parsing |
| `blake3` | 1.x | Snapshot hashing |
| `serde` / `serde_json` | 1.x | JSON-RPC serialization |
| `ciborium` | 0.2 | CBOR canonical encoding |
| `thiserror` / `anyhow` | 2.x / 1.x | Error handling |
| `hex` | 0.4 | Hash hex encoding |
| `wasm-bindgen` | 0.2 | WASM bindings |

---

## License

MIT — see [LICENSE](LICENSE).
