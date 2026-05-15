# Anvil CRDT Relational Engine

> A database that never disagrees, even when nobody is online.

CRDT-native embedded relational engine. Multiple writers mutate locally without coordination. Merges converge. Invariants survive.

## Architecture

```
SQL Layer → Parser → Query Engine → Transaction Layer → CRDT Merge Layer → Replication Layer → Storage Engine
```

### Key Design Decisions

| Property | Choice | Rationale |
|---|---|---|
| Conflict granularity | Cell-level LWW | Preserves concurrent column updates |
| Causality | Lamport clock + peer_id tie-break | O(1) per cell, deterministic |
| Delete semantics | Delete-wins tombstone | Prevents zombie resurrection |
| FK policy | Tombstone (declared: `tombstone`) | Child survives, referential linkage preserved |
| Uniqueness | Reservation/claim protocol | Deterministic convergence, loser preserved |
| Hash | BLAKE3 over canonical CBOR | Fast, deterministic, machine-stable |
| Sync | Frontier-based anti-entropy | O(changed_rows) bandwidth |

## Workspace Crates

| Crate | Responsibility |
|---|---|
| `core` | Shared types: Version, Cell, Row, Tombstone, Frontier, UniquenessClaim |
| `crdt` | Lamport clock, merge semantics, tombstone store, uniqueness registry |
| `storage` | In-memory canonical row store, schema store, serialization |
| `replication` | Per-peer replica state (clock + storage + CRDT state) |
| `sync` | Anti-entropy sync: frontier comparison, delta extraction, reconciliation |
| `query` | SELECT/WHERE/ORDER BY/LIMIT execution |
| `sql` | sqlparser-rs integration, schema + write execution |
| `index` | Deterministic secondary indexes (BTreeMap-based) |
| `transaction` | Local transaction context, row-level atomicity |
| `hashing` | BLAKE3 snapshot hashing, canonical serialization |
| `gc` | Causal-stability-based tombstone GC |
| `metadata` | Peer registry, global frontier tracking |
| `network` | Transport abstraction (Phase 10) |
| `adapter` | Subprocess binary for Python benchmark bridge |
| `wasm-runtime` | WASM/wasm-bindgen bindings (Phase 9) |

## Reference Schema

```sql
CREATE TABLE users (
  id    TEXT PRIMARY KEY,
  email TEXT NOT NULL UNIQUE,
  name  TEXT
);

CREATE TABLE orders (
  id          TEXT PRIMARY KEY,
  user_id     TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  status      TEXT NOT NULL,
  total_cents INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX orders_by_user ON orders(user_id, status);
```

## Build

```bash
# Prerequisites: Rust stable (1.70+)
cargo build --release

# Run tests
cargo test --lib

# Build adapter binary
cargo build --release -p adapter
```

## Benchmark Adapter

```bash
# Python bridge (subprocess)
./adapter/adapter.py --adapter adapters.anvil:Engine --fk-policy tombstone --quick
```

## Implementation Status

| Phase | Status | Description |
|---|---|---|
| Phase 1 | ✅ Complete | Repository bootstrap, workspace, core types, CRDT structures |
| Phase 2 | ✅ Complete | Core CRDT engine (merge, clock, tombstone, uniqueness) |
| Phase 3 | ✅ Skeleton | Storage engine (in-memory) |
| Phase 4 | ✅ Skeleton | Sync engine (anti-entropy) |
| Phase 5 | 🔲 Pending | Secondary indexes |
| Phase 6 | 🔲 Pending | SQL engine |
| Phase 7 | 🔲 Pending | Transactions |
| Phase 8 | 🔲 Pending | Garbage collection |
| Phase 9 | 🔲 Pending | WASM runtime |
| Phase 10 | 🔲 Pending | Networking |
| Phase 11 | 🔲 Pending | Python adapter |
| Phase 12 | 🔲 Pending | Benchmark suite |
| Phase 13 | 🔲 Pending | Release stabilization |

## Dependencies

| Crate | Version | Purpose |
|---|---|---|
| `serde` | 1 | Serialization |
| `serde_json` | 1 | JSON for adapter bridge |
| `ciborium` | 0.2 | Canonical CBOR serialization |
| `blake3` | 1 | Snapshot hashing |
| `thiserror` | 2 | Error types |
| `anyhow` | 1 | Error propagation |
| `sqlparser` | 0.54 | SQL parsing |
| `tokio` | 1 | Async runtime (networking) |
| `hex` | 0.4 | Hash encoding |
| `wasm-bindgen` | 0.2 | WASM bindings |

## CRDT Invariants

All merge operations satisfy:
- **Associativity**: `merge(a, merge(b, c)) == merge(merge(a, b), c)`
- **Commutativity**: `merge(a, b) == merge(b, a)`
- **Idempotency**: `merge(a, a) == a`

## FK Policy

Declared policy: **tombstone**

When a parent row is deleted while a concurrent child insert references it:
- Parent is preserved as a tombstone (logically deleted, referentially live)
- Child row survives with FK reference intact
- Normal queries hide the tombstoned parent
- Internal metadata preserves referential linkage
