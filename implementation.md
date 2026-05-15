# IMPLEMENTATION.md

# CRDT Relational Database — Implementation Specification

---

# 1. Project Overview

This project implements a deterministic distributed relational database using:

- CRDT-based replication
- local-first/offline-first semantics
- pairwise anti-entropy synchronization
- deterministic convergence guarantees
- Rust + WASM runtime

The implementation prioritizes:

1. correctness
2. deterministic convergence
3. bounded metadata
4. replay-safe synchronization
5. modular architecture
6. benchmark compatibility

The system is specifically designed for:
- disconnected peers
- partition-tolerant replication
- deterministic state reconciliation
- randomized synchronization replay
- distributed systems benchmarking

---

# 2. High-Level System Architecture

```text
Client/API
    ↓
SQL Layer
    ↓
Parser
    ↓
Query Engine
    ↓
Transaction Layer
    ↓
CRDT Merge Layer
    ↓
Replication Layer
    ↓
Storage Engine
```

---

# 3. Repository Structure

```text
root/
├── Cargo.toml
├── README.md
├── CHANGES.md
├── IMPLEMENTATION.md
├── design_choices.md
├── docs/
├── tests/
├── benches/
├── scripts/
├── adapter/
├── wasm/
└── crates/
    ├── core/
    ├── crdt/
    ├── storage/
    ├── replication/
    ├── sync/
    ├── query/
    ├── sql/
    ├── index/
    ├── transaction/
    ├── hashing/
    ├── gc/
    ├── metadata/
    ├── network/
    ├── adapter/
    └── wasm-runtime/
```

---

# 4. Mandatory Engineering Constraints

## Determinism

ALL logic MUST be deterministic.

Never rely on:
- HashMap iteration order
- wall-clock timestamps
- async execution ordering
- OS scheduling behavior
- runtime-specific serialization order

Always use:
- BTreeMap
- canonical sorting
- stable serialization
- deterministic merge ordering

---

## Metadata Constraints

Metadata MUST remain bounded.

Allowed:
- Lamport clocks
- peer frontier metadata

Forbidden:
- unbounded vector clocks
- append-only operation logs
- per-write causal histories

Target complexity:

```text
O(writers_per_row)
```

NOT:

```text
O(total_operations)
```

---

## Synchronization Constraints

Synchronization MUST be:
- replay-safe
- idempotent
- deterministic
- partition-tolerant

Repeated synchronization MUST converge.

---

## Availability Constraints

Local writes MUST:
- succeed offline
- never require central coordination
- never block on remote peers

---

# 5. Core Runtime Dependencies

Recommended crates:

```toml
serde
serde_cbor
blake3
thiserror
anyhow
sqlparser
tokio
uuid
indexmap
wasm-bindgen
pyo3
```

---

# 6. Core Data Structures

## Version

```rust
pub struct Version {
    pub counter: u64,
    pub peer_id: PeerId,
}
```

Rules:
- larger counter wins
- equal counters resolved by deterministic peer_id ordering

---

## Cell

```rust
pub struct Cell {
    pub value: Value,
    pub version: Version,
}
```

---

## Row

```rust
pub struct Row {
    pub id: RowId,
    pub cells: BTreeMap<ColumnId, Cell>,
    pub deleted: bool,
}
```

---

## Frontier

```rust
pub type Frontier = BTreeMap<PeerId, u64>;
```

---

## Tombstone

```rust
pub struct Tombstone {
    pub row_id: RowId,
    pub version: Version,
}
```

---

## UniquenessClaim

```rust
pub struct UniquenessClaim {
    pub value: String,
    pub owner_row: RowId,
    pub version: Version,
}
```

---

# 7. Merge Invariants

ALL merges MUST satisfy:

## Associativity

```text
merge(a, merge(b, c))
==
merge(merge(a, b), c)
```

---

## Commutativity

```text
merge(a, b)
==
merge(b, a)
```

---

## Idempotency

```text
merge(a, a)
==
a
```

These guarantees are REQUIRED for:
- replay-safe sync
- duplicate-message tolerance
- randomized sync-order convergence
- anti-entropy correctness

---

# 8. Snapshot State Requirements

snapshot_state() MUST:
- return deterministic table ordering
- return rows sorted by PK ascending
- return deterministic column ordering
- exclude tombstoned rows from normal visibility
- preserve internal FK semantics
- exclude runtime-only metadata

Snapshot hashes MUST be:
- bit-identical across replicas
- independent of sync ordering
- independent of runtime memory layout

---

# 9. Required Phases

---

# PHASE 1 — Repository Bootstrap

## Branch

```bash
feature/repository-bootstrap
```

---

## Goals

Create:
- cargo workspace
- module skeleton
- CI setup
- deterministic utility layer
- documentation foundation

---

## Sub-Tasks

### 1.1 Cargo Workspace

Create:
- root Cargo.toml
- workspace crates
- shared types crate

Commit:

```text
[feat] bootstrap: initialize cargo workspace
```

---

### 1.2 Tooling

Configure:
- rustfmt
- clippy
- cargo deny
- CI workflows

Commit:

```text
[feat] tooling: configure formatting linting and CI
```

---

### 1.3 Documentation

Create:
- README.md
- CHANGES.md
- IMPLEMENTATION.md
- design_choices.md

Commit:

```text
[docs] bootstrap: initialize project documentation
```

---

### 1.4 Deterministic Utilities

Implement:
- canonical ordering helpers
- serialization utilities
- deterministic hashing helpers

Commit:

```text
[feat] utils: implement deterministic utilities
```

---

## Required Tests

- workspace compilation
- formatting validation
- lint validation

---

# PHASE 2 — Core CRDT Engine

## Branch

```bash
feature/core-crdt-engine
```

---

## Goals

Implement:
- Version
- Cell
- Row
- Lamport clocks
- merge semantics
- tombstones
- uniqueness claims

---

## Sub-Tasks

### 2.1 Core Types

Implement:
- Version
- Cell
- Row
- Tombstone
- Frontier

Commit:

```text
[feat] crdt: implement core CRDT structures
```

---

### 2.2 Lamport Clock Engine

Implement:
- counter increments
- frontier tracking
- synchronization updates

Commit:

```text
[feat] crdt: implement Lamport clock engine
```

---

### 2.3 Cell Merge Semantics

Implement:
- deterministic merge ordering
- peer_id tie-breaks

Commit:

```text
[feat] crdt: implement deterministic cell merge semantics
```

---

### 2.4 Tombstone Semantics

Implement:
- delete-wins visibility
- preserved tombstone state

Commit:

```text
[feat] crdt: implement tombstone semantics
```

---

### 2.5 Uniqueness Claims

Implement:
- reservation protocol
- deterministic conflict resolution
- conflict metadata

Commit:

```text
[feat] crdt: implement uniqueness claim protocol
```

---

## Required Tests

- merge associativity
- merge commutativity
- merge idempotency
- deterministic tie-break validation
- uniqueness convergence tests

---

# PHASE 3 — Storage Engine

## Branch

```bash
feature/storage-engine
```

---

## Goals

Implement:
- canonical persistence
- metadata persistence
- deterministic serialization
- recovery semantics

---

## Sub-Tasks

### 3.1 Canonical Row Storage

Implement:
- row persistence
- stable row ordering

Commit:

```text
[feat] storage: implement canonical row persistence
```

---

### 3.2 Metadata Persistence

Implement:
- frontier persistence
- tombstone persistence
- uniqueness metadata persistence

Commit:

```text
[feat] storage: persist synchronization metadata
```

---

### 3.3 Canonical Serialization

Implement:
- deterministic CBOR serialization

Commit:

```text
[feat] storage: implement canonical serialization
```

---

### 3.4 Recovery Semantics

Implement:
- startup recovery
- corruption handling

Commit:

```text
[feat] storage: implement recovery semantics
```

---

## Required Tests

- deterministic serialization tests
- persistence roundtrip tests
- recovery correctness tests

---

# PHASE 4 — Replication & Sync Engine

## Branch

```bash
feature/sync-engine
```

---

## Goals

Implement:
- anti-entropy synchronization
- frontier comparison
- delta extraction
- replay-safe reconciliation

---

## Sub-Tasks

### 4.1 Frontier Comparison

Implement:
- missing delta detection

Commit:

```text
[feat] sync: implement frontier comparison
```

---

### 4.2 Delta Extraction

Implement:
- changed-row extraction
- delta payload generation

Commit:

```text
[feat] sync: implement delta extraction
```

---

### 4.3 Sync Sessions

Implement:
- handshake protocol
- frontier exchange
- replay-safe synchronization

Commit:

```text
[feat] sync: implement sync session protocol
```

---

### 4.4 Merge Reconciliation

Implement:
- deterministic reconciliation
- replay-safe merge application

Commit:

```text
[feat] sync: implement deterministic reconciliation
```

---

### 4.5 Snapshot Hash Validation

Implement:
- convergence verification
- deterministic snapshot hashing

Commit:

```text
[feat] hashing: implement deterministic convergence hashing
```

---

## Required Tests

- delayed synchronization
- duplicate synchronization
- randomized sync ordering
- replay safety
- convergence validation

---

# PHASE 5 — Secondary Indexes

## Branch

```bash
feature/index-engine
```

---

## Goals

Implement:
- deterministic indexes
- derived index maintenance
- deterministic range queries

---

## Sub-Tasks

### 5.1 Index Structures

Implement:
- ordered indexes
- canonical traversal

Commit:

```text
[feat] index: implement deterministic index structures
```

---

### 5.2 Incremental Updates

Implement:
- index rebuild/update logic

Commit:

```text
[feat] index: implement incremental index maintenance
```

---

### 5.3 Range Queries

Implement:
- deterministic scans

Commit:

```text
[feat] index: implement deterministic range scans
```

---

## Required Tests

- deterministic ordering tests
- index convergence tests
- replay-safe index rebuild tests

---

# PHASE 6 — SQL Engine

## Branch

```bash
feature/sql-engine
```

---

## Goals

Implement:
- SQL parsing
- deterministic query execution
- local relational semantics

---

## Sub-Tasks

### 6.1 SQL Parser Integration

Implement:
- sqlparser-rs integration

Commit:

```text
[feat] sql: integrate sqlparser-rs
```

---

### 6.2 Schema Engine

Implement:
- CREATE TABLE
- CREATE INDEX
- UNIQUE
- FOREIGN KEY

Commit:

```text
[feat] sql: implement schema engine
```

---

### 6.3 Query Execution

Implement:
- SELECT
- WHERE
- ORDER BY
- LIMIT

Commit:

```text
[feat] sql: implement deterministic query execution
```

---

### 6.4 Write Execution

Implement:
- INSERT
- UPDATE
- DELETE

Commit:

```text
[feat] sql: implement write execution
```

---

## Required Tests

- deterministic query results
- schema correctness
- FK behavior
- uniqueness behavior

---

# PHASE 7 — Transactions

## Branch

```bash
feature/transaction-engine
```

---

## Goals

Implement:
- local transactions
- row-level atomicity
- rollback handling

---

## Sub-Tasks

### 7.1 Transaction Context

Implement:
- begin
- commit
- rollback

Commit:

```text
[feat] txn: implement transaction context
```

---

### 7.2 Atomic Local Writes

Implement:
- atomic row persistence

Commit:

```text
[feat] txn: implement atomic local writes
```

---

### 7.3 Recovery Safety

Implement:
- crash-safe recovery semantics

Commit:

```text
[feat] txn: implement recovery semantics
```

---

## Required Tests

- rollback correctness
- crash recovery validation
- partial write handling

---

# PHASE 8 — Garbage Collection

## Branch

```bash
feature/tombstone-gc
```

---

## Goals

Implement:
- causal stability checks
- tombstone cleanup
- metadata compaction

---

## Sub-Tasks

### 8.1 Stability Tracking

Implement:
- frontier stabilization

Commit:

```text
[feat] gc: implement causal stability tracking
```

---

### 8.2 Tombstone Reclamation

Implement:
- safe tombstone deletion

Commit:

```text
[feat] gc: implement tombstone reclamation
```

---

### 8.3 Metadata Compaction

Implement:
- obsolete metadata cleanup

Commit:

```text
[feat] gc: implement metadata compaction
```

---

## Required Tests

- no resurrection validation
- delayed sync safety
- replay safety after GC

---

# PHASE 9 — WASM Runtime

## Branch

```bash
feature/wasm-runtime
```

---

## Goals

Implement:
- WASM compilation
- browser runtime
- local browser persistence

---

## Sub-Tasks

### 9.1 WASM Bindings

Implement:
- wasm-bindgen integration

Commit:

```text
[feat] wasm: implement wasm bindings
```

---

### 9.2 Browser Persistence

Implement:
- IndexedDB/local storage layer

Commit:

```text
[feat] wasm: implement browser persistence
```

---

### 9.3 JavaScript APIs

Implement:
- JS interop APIs

Commit:

```text
[feat] wasm: implement JavaScript APIs
```

---

## Required Tests

- browser execution
- deterministic WASM behavior
- persistence recovery

---

# PHASE 10 — Networking

## Branch

```bash
feature/networking
```

---

## Goals

Implement:
- transport abstraction
- peer communication
- sync transport protocol

---

## Sub-Tasks

### 10.1 Transport Interfaces

Implement:
- websocket/WebRTC abstraction

Commit:

```text
[feat] network: implement transport abstraction
```

---

### 10.2 Sync Payload Transport

Implement:
- sync transport protocol

Commit:

```text
[feat] network: implement sync transport
```

---

### 10.3 Peer Sessions

Implement:
- peer lifecycle management

Commit:

```text
[feat] network: implement peer session handling
```

---

## Required Tests

- delayed transport
- duplicate transport replay
- partial synchronization recovery

---

# PHASE 11 — Python Adapter Integration

## Branch

```bash
feature/python-adapter
```

---

## Goals

Implement:
- benchmark-compatible adapter
- deterministic snapshot bridge
- Rust ↔ Python integration

---

## Sub-Tasks

### 11.1 Adapter Interface

Implement:
- open_peer()
- apply_schema()
- execute()
- sync()
- snapshot_hash()
- snapshot_state()
- close()

Commit:

```text
[feat] adapter: implement benchmark adapter interface
```

---

### 11.2 Rust Bridge

Implement:
- pyo3 bridge OR subprocess bridge

Commit:

```text
[feat] adapter: implement Rust bridge layer
```

---

### 11.3 Harness Validation

Implement:
- self_check.py compatibility
- run.py compatibility

Commit:

```text
[test] adapter: validate benchmark harness compatibility
```

---

## Required Tests

- deterministic snapshot validation
- replay-safe sync validation
- harness compatibility tests

---

# PHASE 12 — Benchmark & Validation Suite

## Branch

```bash
feature/benchmark-suite
```

---

## Goals

Implement:
- randomized convergence testing
- partition simulation
- deterministic replay validation

---

## Sub-Tasks

### 12.1 Randomized Sync Simulation

Implement:
- random sync replay engine

Commit:

```text
[test] benchmark: implement randomized sync simulation
```

---

### 12.2 Partition Simulation

Implement:
- partition recovery tests

Commit:

```text
[test] benchmark: implement partition simulation
```

---

### 12.3 Snapshot Validation

Implement:
- deterministic convergence verification

Commit:

```text
[test] benchmark: implement snapshot convergence validation
```

---

### 12.4 Metrics

Implement:
- metadata metrics
- sync metrics
- convergence metrics

Commit:

```text
[feat] benchmark: implement benchmark metrics
```

---

# PHASE 13 — Release Stabilization

## Branch

```bash
feature/release-stabilization
```

---

## Goals

Finalize:
- documentation
- examples
- release validation
- deployment instructions

---

## Sub-Tasks

### 13.1 Final README

Commit:

```text
[docs] release: finalize README
```

---

### 13.2 Architecture Documentation

Commit:

```text
[docs] release: finalize architecture documentation
```

---

### 13.3 Example Workflows

Commit:

```text
[docs] release: add workflow examples
```

---

### 13.4 Release Validation

Commit:

```text
[test] release: finalize validation suite
```

---

# 10. Final Engineering Requirements

At the end of EVERY sub-phase:

The coding agent MUST:
1. ensure compilation succeeds
2. ensure tests pass
3. update README.md
4. update CHANGES.md
5. update IMPLEMENTATION.md
6. document unfinished work
7. document known limitations
8. create isolated commit
9. maintain deterministic behavior

The repository MUST always remain:
- buildable
- testable
- replay-safe
- resumable by another agent
- deterministically reproducible