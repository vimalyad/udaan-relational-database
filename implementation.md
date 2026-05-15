# IMPLEMENTATION.md

# CRDT Relational Database — Implementation Specification

## Project Overview

This project implements a deterministic distributed relational database using:

- CRDT-based replication
- local-first/offline-first semantics
- pairwise anti-entropy synchronization
- deterministic convergence guarantees
- Rust + WASM architecture

The system is designed specifically for:
- disconnected peers
- deterministic state convergence
- bounded metadata
- conflict-preserving replication
- distributed systems benchmarking

The implementation MUST prioritize:
1. correctness
2. determinism
3. reproducibility
4. bounded metadata growth
5. modularity
6. benchmark alignment

---

# Core Architectural Goals

The database MUST:

- support relational semantics
- support disconnected local writes
- synchronize using pairwise anti-entropy
- converge deterministically
- avoid centralized coordination
- preserve causal history
- support bounded metadata growth
- support deterministic hashing
- support browser + native runtimes via WASM

---

# Fundamental Design Decisions

## Database Model
- relational database
- CRDT-based replication
- local-first/offline-first

## Conflict Resolution
- cell-level CRDT semantics
- NOT row-level LWW

## Versioning
- Lamport logical clocks
- deterministic peer_id tie-breaks

## Deletes
- tombstone-based delete-wins visibility semantics

## Synchronization
- frontier-based anti-entropy synchronization

## Reads
- local canonical replica reads only

## Storage
- materialized canonical row storage

## Transactions
- local replica atomicity only

## Foreign Keys
- tombstone FK semantics

## Uniqueness
- reservation/claim-based uniqueness protocol

## Indexes
- deterministic derived indexes

## Metadata Constraints
- bounded metadata only
- no unbounded vector clocks
- no append-only causal histories

## Runtime
- Rust + WASM

---

# Non-Goals

The system intentionally DOES NOT implement:

- distributed ACID transactions
- global serializable isolation
- realtime op-based replication
- centralized coordination
- full PostgreSQL compatibility
- distributed joins
- recursive query planning
- wall-clock conflict resolution
- unbounded operation logs

---

# Determinism Requirements

ALL system behavior MUST be deterministic.

This includes:
- serialization
- merge ordering
- index ordering
- snapshot hashing
- conflict resolution
- sync reconciliation

---

# Determinism Rules

NEVER rely on:
- HashMap iteration order
- wall-clock timestamps
- OS scheduling order
- async race order
- runtime-specific serialization order

ALWAYS use:
- BTreeMap
- canonical sorting
- explicit ordering
- stable serialization

---

# Metadata Bound Requirements

Metadata MUST remain bounded.

Allowed:
- Lamport clocks
- peer frontier metadata

Forbidden:
- per-operation vector clocks
- unbounded version histories
- append-only causal DAGs

Metadata complexity target:
```text
O(writers_per_row)
```

NOT:
```text
O(total_operations)
```

---

# Convergence Guarantee

All replicas MUST converge to:
- identical rows
- identical indexes
- identical tombstones
- identical metadata
- identical snapshot hashes

independent of:
- sync order
- delayed synchronization
- replay order
- duplicate messages
- network partitions

---

# Repository Structure

```text
src/
├── sql/
├── parser/
├── query/
├── storage/
├── crdt/
├── replication/
├── sync/
├── index/
├── transaction/
├── gc/
├── hashing/
├── metadata/
├── network/
└── tests/
```

---

# Module Responsibilities

## parser/
Responsible for:
- SQL parsing
- AST generation
- parser integration

Recommended:
- sqlparser-rs

---

## query/
Responsible for:
- SELECT execution
- filtering
- sorting
- LIMIT
- joins
- deterministic local reads

---

## storage/
Responsible for:
- persistence
- serialization
- row materialization
- metadata persistence

MUST NOT contain:
- CRDT merge logic

---

## crdt/
Responsible for:
- cell merge semantics
- Lamport ordering
- tombstone semantics
- uniqueness resolution
- deterministic conflict resolution

This is the CORE database logic.

---

## replication/
Responsible for:
- frontier tracking
- delta extraction
- reconciliation orchestration

---

## sync/
Responsible for:
- sync sessions
- peer handshake
- frontier exchange
- transport abstraction

---

## index/
Responsible for:
- derived indexes
- deterministic ordering
- rebuild/update logic

---

## transaction/
Responsible for:
- local transaction batching
- commit/rollback
- row-level atomicity

---

## gc/
Responsible for:
- tombstone garbage collection
- metadata cleanup
- causal stability checks

---

## hashing/
Responsible for:
- canonical serialization
- deterministic hashing
- snapshot verification

---

## metadata/
Responsible for:
- peer metadata
- schema metadata
- frontier metadata
- sync metadata

---

## network/
Responsible for:
- websocket/WebRTC abstraction
- browser transport
- sync transport APIs

MUST NOT contain:
- CRDT merge logic

---

# Core Data Structures

## Version

```rust
struct Version {
    counter: u64,
    peer_id: PeerId,
}
```

Rules:
- larger counter wins
- equal counters resolved via deterministic peer_id ordering

---

## Cell

```rust
struct Cell {
    value: Value,
    version: Version,
}
```

---

## Row

```rust
struct Row {
    id: RowId,
    cells: BTreeMap<ColumnId, Cell>,
    deleted: bool,
}
```

---

## Frontier

```rust
type Frontier = BTreeMap<PeerId, u64>;
```

Tracks:
- maximum observed Lamport counters per peer

---

# Delete Semantics

Deletes are:
- tombstone-based
- delete-wins for visibility

Delete MUST:
- preserve row physically
- preserve merge history
- prevent resurrection

Queries MUST:
- exclude tombstoned rows by default

---

# FK Semantics

Foreign keys use:
- tombstone semantics

Behavior:
- child survives parent deletion
- parent becomes tombstoned
- referential linkage preserved

---

# Uniqueness Semantics

Uniqueness is implemented using:
- reservation/claim-based protocol

Concurrent conflicts:
- resolved deterministically
- losing rows preserved
- conflict metadata retained

No silent deletion allowed.

---

# Index Semantics

Indexes are:
- derived state
- deterministic
- rebuildable

Indexes MUST:
- converge deterministically
- exclude tombstoned rows
- use canonical ordering

---

# Snapshot Hashing

Snapshot hashing MUST:
- produce identical hashes on converged replicas
- ignore runtime ordering artifacts

Recommended hash:
- BLAKE3

Serialization MUST be:
- canonical
- deterministic
- stable

---

# Sync Semantics

Synchronization is:
- pairwise
- anti-entropy
- frontier-based

Sync MUST:
- exchange only deltas
- support replay safety
- support delayed synchronization
- support duplicate messages

---

# Transaction Semantics

Transactions are:
- local only
- row-level atomic

NOT supported:
- distributed serializable isolation
- distributed ACID

---

# SQL Scope

Supported:
- CREATE TABLE
- CREATE INDEX
- PRIMARY KEY
- UNIQUE
- FOREIGN KEY
- INSERT
- UPDATE
- DELETE
- SELECT
- WHERE
- ORDER BY
- LIMIT

Optional:
- INNER JOIN

Not supported initially:
- recursive queries
- distributed joins
- triggers
- stored procedures
- query optimization engine

---

# Serialization Format

Recommended:
- CBOR

Requirements:
- deterministic ordering
- stable encoding
- canonical serialization

---

# Required Crates

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
```

---

# Engineering Standards

Mandatory:
- cargo fmt
- cargo clippy
- stable Rust
- no unwrap() in critical paths
- explicit error handling
- deterministic collections

---

# REQUIRED IMPLEMENTATION PHASES

# PHASE 1 — Repository Bootstrap

## Branch

```bash
feature/repository-bootstrap
```

## Tasks

### 1.1 Workspace Setup
Create:
- cargo workspace
- module crates
- shared types crate

Commit:
```text
[feat] bootstrap: initialize cargo workspace structure
```

---

### 1.2 Tooling Setup
Configure:
- rustfmt
- clippy
- cargo deny
- CI workflows

Commit:
```text
[feat] tooling: configure linting formatting and CI
```

---

### 1.3 Documentation Bootstrap
Create:
- README.md
- CHANGES.md
- IMPLEMENTATION.md

Commit:
```text
[docs] bootstrap: add initial project documentation
```

---

### 1.4 Deterministic Utility Layer
Implement:
- canonical sorting helpers
- deterministic serialization utilities

Commit:
```text
[feat] utils: add deterministic ordering utilities
```

---

# PHASE 2 — Core CRDT Engine

## Branch

```bash
feature/core-crdt-engine
```

## Tasks

### 2.1 Core Types
Implement:
- Row
- Cell
- Version
- PeerId
- Tombstone

Commit:
```text
[feat] crdt: implement core CRDT data structures
```

---

### 2.2 Lamport Engine
Implement:
- Lamport clocks
- counter updates
- frontier tracking

Commit:
```text
[feat] crdt: implement Lamport clock engine
```

---

### 2.3 Cell Merge Semantics
Implement:
- deterministic merge ordering
- concurrent update resolution

Commit:
```text
[feat] crdt: implement deterministic cell merge semantics
```

---

### 2.4 Tombstone Semantics
Implement:
- delete-wins visibility
- preserved tombstones

Commit:
```text
[feat] crdt: implement tombstone delete semantics
```

---

### 2.5 Uniqueness Claims
Implement:
- reservation protocol
- conflict metadata
- deterministic winners

Commit:
```text
[feat] crdt: implement uniqueness claim protocol
```

---

# PHASE 3 — Storage Engine

## Branch

```bash
feature/storage-engine
```

## Tasks

### 3.1 Canonical Row Storage
Implement:
- row persistence
- deterministic layout

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
[feat] storage: persist replication metadata
```

---

### 3.3 Deterministic Serialization
Implement:
- canonical CBOR serialization

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

# PHASE 4 — Replication & Sync Engine

## Branch

```bash
feature/sync-engine
```

## Tasks

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

Commit:
```text
[feat] sync: implement delta extraction
```

---

### 4.3 Sync Sessions
Implement:
- handshake protocol
- frontier exchange

Commit:
```text
[feat] sync: implement sync session protocol
```

---

### 4.4 Merge Application
Implement:
- deterministic reconciliation

Commit:
```text
[feat] sync: implement deterministic merge reconciliation
```

---

### 4.5 Snapshot Hash Validation
Implement:
- convergence verification

Commit:
```text
[feat] hashing: implement snapshot convergence validation
```

---

# PHASE 5 — Secondary Indexes

## Branch

```bash
feature/index-engine
```

## Tasks

### 5.1 Index Structures
Implement:
- deterministic indexes

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
[feat] index: implement incremental index updates
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

# PHASE 6 — SQL Engine

## Branch

```bash
feature/sql-engine
```

## Tasks

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
- FK

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
[feat] sql: implement write execution layer
```

---

# PHASE 7 — Transactions

## Branch

```bash
feature/transaction-engine
```

## Tasks

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
- row-level atomicity

Commit:
```text
[feat] txn: implement atomic local writes
```

---

### 7.3 Recovery Safety
Implement:
- crash-safe local transaction recovery

Commit:
```text
[feat] txn: implement transaction recovery semantics
```

---

# PHASE 8 — Garbage Collection

## Branch

```bash
feature/tombstone-gc
```

## Tasks

### 8.1 Frontier Stability
Implement:
- causal stability checks

Commit:
```text
[feat] gc: implement causal stability tracking
```

---

### 8.2 Tombstone Reclamation
Implement:
- safe tombstone cleanup

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

# PHASE 9 — WASM Runtime

## Branch

```bash
feature/wasm-runtime
```

## Tasks

### 9.1 WASM Bindings
Implement:
- wasm-bindgen integration

Commit:
```text
[feat] wasm: implement wasm bindings
```

---

### 9.2 Browser Storage
Implement:
- IndexedDB/local persistence

Commit:
```text
[feat] wasm: implement browser persistence layer
```

---

### 9.3 JS Interop
Implement:
- JS-facing APIs

Commit:
```text
[feat] wasm: implement JavaScript interop APIs
```

---

# PHASE 10 — Networking

## Branch

```bash
feature/networking
```

## Tasks

### 10.1 Transport Abstraction
Implement:
- transport interfaces

Commit:
```text
[feat] network: implement transport abstraction
```

---

### 10.2 Sync Protocol
Implement:
- sync payload transport

Commit:
```text
[feat] network: implement sync transport protocol
```

---

### 10.3 Peer Sessions
Implement:
- peer communication lifecycle

Commit:
```text
[feat] network: implement peer session handling
```

---

# PHASE 11 — Benchmark Suite

## Branch

```bash
feature/benchmark-suite
```

## Tasks

### 11.1 Randomized Sync Simulation
Implement:
- randomized sync replay testing

Commit:
```text
[test] benchmark: implement randomized sync simulation
```

---

### 11.2 Partition Simulation
Implement:
- network partition testing

Commit:
```text
[test] benchmark: implement partition simulation tests
```

---

### 11.3 Snapshot Validation
Implement:
- deterministic convergence verification

Commit:
```text
[test] benchmark: implement convergence hash validation
```

---

### 11.4 Metrics
Implement:
- metadata metrics
- sync metrics
- convergence metrics

Commit:
```text
[feat] benchmark: implement benchmark metrics
```

---

# PHASE 12 — Release Stabilization

## Branch

```bash
feature/release-stabilization
```

## Tasks

### 12.1 Final README
Finalize:
- examples
- usage
- architecture diagrams

Commit:
```text
[docs] release: finalize README
```

---

### 12.2 Architecture Docs
Finalize:
- internal protocol docs
- sync semantics
- invariants

Commit:
```text
[docs] release: finalize architecture documentation
```

---

### 12.3 Examples
Add:
- sync demos
- convergence examples
- partition examples

Commit:
```text
[docs] release: add example workflows
```

---

### 12.4 Release Validation
Finalize:
- full convergence testing
- CI stabilization

Commit:
```text
[test] release: finalize validation suite
```

---

# REQUIRED TESTING STRATEGY

Every feature MUST include:

## Unit Tests
- merge semantics
- Lamport logic
- tombstones
- serialization
- hashing

## Integration Tests
- sync replay
- delayed sync
- duplicate sync
- partition recovery

## Property Tests
- associativity
- commutativity
- idempotency

## Determinism Tests
- identical hashes
- identical indexes
- deterministic serialization

---

# REQUIRED FINAL OUTPUT

The final system MUST provide:

- deterministic CRDT relational database
- local-first execution
- offline-first replication
- pairwise anti-entropy synchronization
- deterministic convergence
- bounded metadata
- reproducible snapshot hashing
- Rust + WASM runtime
- deterministic relational queries
- benchmark-ready convergence proofs