# CHANGES.md

## v1.0.0 — Release Stabilization

**Benchmark score: 1.00 / 1.00**

All 13 phases complete. Full CRDT-native relational engine with SQL interface, Python subprocess adapter, and validated convergence across all scenarios.

| Phase | Description |
|---|---|
| Phase 1 | Repository bootstrap, workspace layout, core shared types, initial CRDT structures |
| Phase 2 | Core CRDT engine: Lamport clock, cell/row merge semantics, tombstone store, uniqueness registry |
| Phase 3 | In-memory storage engine, schema store, canonical CBOR serialization |
| Phase 4 | Anti-entropy sync engine: frontier comparison, delta extraction, delta application |
| Phase 5 | Secondary indexes: deterministic BTreeMap-based index structures, rebuild and update helpers |
| Phase 6 | SQL executor: CREATE TABLE, CREATE INDEX, INSERT, UPDATE, DELETE, SELECT with WHERE/ORDER BY/LIMIT |
| Phase 7 | Local transaction context with row-level atomicity and rollback |
| Phase 8 | Tombstone GC based on causal stability across all known peers |
| Phase 9 | WASM runtime bindings via wasm-bindgen |
| Phase 10 | Network transport abstraction layer |
| Phase 11 | Python subprocess adapter (JSON-RPC bridge), adapter.py Engine class |
| Phase 12 | Benchmark suite: randomized sync, partition simulation, convergence validation |
| Phase 13 | Release stabilization: README, ARCHITECTURE.md, EXAMPLES.md, CHANGES.md, final validation |

---

## Phase 13 — Release Stabilization

**Branch:** `feature/python-adapter`

### What Changed

- Rewrote `README.md` with full architecture overview, crate map, quick start, and CRDT semantics summary
- Created `ARCHITECTURE.md` covering data model, merge invariants, sync protocol, uniqueness protocol, snapshot hashing, SQL layer, and Python adapter protocol
- Created `EXAMPLES.md` with five annotated workflow examples (basic sync, LWW resolution, uniqueness conflict, tombstone semantics, benchmark self-check)
- Updated `CHANGES.md` with full 13-phase history and v1.0.0 release section
- Confirmed `cargo test --lib` passes all tests
- Confirmed `cargo build --release -p adapter` succeeds

### Benchmark Score

**1.00 / 1.00** — all validation scenarios pass

---

## Phase 12 — Benchmark Suite

**Branch:** `feature/python-adapter`

### What Changed

- Implemented randomized multi-peer sync validation (`crates/benchmark/tests/randomized_sync.rs`)
- Implemented partition simulation test (`crates/benchmark/tests/partition_simulation.rs`)
- Implemented snapshot hash convergence validation (`crates/benchmark/tests/snapshot_validation.rs`)
- Validated benchmark score: 1.00/1.00

---

## Phase 11 — Python Subprocess Adapter

**Branch:** `feature/python-adapter`

### What Changed

- Implemented `adapter/adapter.py` — Python `Engine` class bridging the benchmark harness to the Rust subprocess
- Implemented `crates/adapter/src/engine.rs` — JSON-RPC command dispatcher (OpenPeer, ApplySchema, Execute, Sync, SnapshotHash, SnapshotState, Close)
- Implemented `crates/adapter/src/main.rs` — stdin/stdout newline-delimited JSON loop
- Integrated SQL executor, sync engine, and snapshot hasher into a single multi-peer in-process state

---

## Phase 10 — Network Transport

**Branch:** `feature/python-adapter`

### What Changed

- Implemented `crates/network` transport abstraction layer
- In-process transport for testing; async transport stub for future extension

---

## Phase 9 — WASM Runtime

**Branch:** `feature/python-adapter`

### What Changed

- Implemented `crates/wasm-runtime` with wasm-bindgen bindings
- Exposed core engine operations to JavaScript/WASM environments

---

## Phase 8 — Tombstone GC

**Branch:** `feature/python-adapter`

### What Changed

- Implemented `crates/gc` — causal-stability-based tombstone garbage collection
- A tombstone is eligible for GC once every known peer's frontier has advanced past the tombstone's version

---

## Phase 7 — Transactions

**Branch:** `feature/python-adapter`

### What Changed

- Implemented `crates/transaction` — local transaction context with row-level atomicity
- Buffered write set; commit applies all changes atomically; rollback discards the buffer

---

## Phase 6 — SQL Executor

**Branch:** `feature/python-adapter`

### What Changed

- Implemented full SQL execution in `crates/sql/src/executor.rs`
- Supported statements: CREATE TABLE, CREATE INDEX, INSERT INTO ... VALUES, UPDATE ... SET ... WHERE, DELETE FROM ... WHERE, SELECT with projections, WHERE filters, ORDER BY, and LIMIT
- FK tombstone policy: child insert succeeds even if parent row is tombstoned
- Uniqueness claims registered on INSERT and UPDATE for UNIQUE columns
- Query result type (`crates/query`) returns column names + typed value rows

---

## Phase 5 — Secondary Indexes

**Branch:** `feature/python-adapter`

### What Changed

- Implemented `crates/index` — deterministic BTreeMap-based secondary index structures
- `IndexManager` maintains per-index BTree keyed by (column values, row_id)
- `rebuild_table` reconstructs an index from current storage state
- `update_row` applies incremental index changes on INSERT/UPDATE/DELETE

---

## Phase 4 — Sync Engine

**Branch:** `feature/repository-bootstrap`

### What Changed

- Implemented `extract_delta(source, remote_frontier)` — O(changed_rows) delta extraction
- Implemented `apply_delta(target, delta)` — idempotent, order-independent delta application
- Implemented `SyncSession` for coordinating bidirectional peer sync

---

## Phase 3 — Storage Engine

**Branch:** `feature/repository-bootstrap`

### What Changed

- Implemented `crates/storage/src/engine.rs` — in-memory canonical row store
- `visible_rows` iterator returns only non-tombstoned rows
- `snapshot_table` returns a full BTreeMap snapshot for hashing
- Implemented `crates/storage/src/schema_store.rs` — per-peer schema registry
- Implemented `crates/storage/src/serialization.rs` — canonical CBOR serialization helpers

---

## Phase 2 — Core CRDT Engine

**Branch:** `feature/repository-bootstrap`

### What Changed

- Implemented Lamport clock with `tick()` and `update_from_frontier()` (`crates/crdt/src/clock.rs`)
- Implemented cell-level merge (`merge_cell`) and row-level merge (`merge_row`) (`crates/crdt/src/merge.rs`)
- Implemented tombstone store with causal-stability check (`crates/crdt/src/tombstone.rs`)
- Implemented uniqueness reservation/claim protocol with loser preservation (`crates/crdt/src/uniqueness.rs`)
- Implemented per-peer `ReplicaState` combining clock, storage, CRDT state, and frontier (`crates/replication`)

---

## Phase 1 — Repository Bootstrap

**Branch:** `feature/repository-bootstrap`

### What Changed

- Initialized Cargo workspace with 14 crates
- Implemented core shared types: `Version`, `Cell`, `Row`, `Tombstone`, `Frontier`, `UniquenessClaim`, `SyncDelta`, `TableSchema`
- Implemented Lamport clock engine (`crdt::clock`)
- Implemented deterministic cell/row/table merge semantics (`crdt::merge`)
- Implemented tombstone store with causal-stability GC (`crdt::tombstone`)
- Implemented uniqueness reservation/claim protocol (`crdt::uniqueness`)
- Implemented in-memory storage engine (`storage::engine`)
- Implemented schema store (`storage::schema_store`)
- Implemented canonical CBOR serialization helpers (`storage::serialization`)
- Implemented anti-entropy sync skeleton (`sync`)
- Implemented BLAKE3 snapshot hasher (`hashing`)
- Implemented deterministic secondary index structures (`index`)
- Implemented local transaction buffer (`transaction`)
- Implemented tombstone GC logic (`gc`)
- Implemented peer metadata registry (`metadata`)
- Implemented subprocess adapter binary skeleton (`adapter`)
- Configured rustfmt

### Tests

- 11 unit tests passing: cell merge invariants (associativity, commutativity, idempotency), uniqueness claim protocol
