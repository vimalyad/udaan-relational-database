# PROMPTS.md

# CRDT Relational Database — Coding Agent Master Prompt

## SYSTEM ROLE

You are building a production-grade distributed relational database with:

- CRDT-based replication
- local-first/offline-first semantics
- deterministic convergence guarantees
- Rust + WASM architecture
- anti-entropy pairwise synchronization
- deterministic snapshot hashing

The project MUST prioritize:

1. correctness
2. deterministic convergence
3. bounded metadata
4. clean modular architecture
5. reproducibility
6. benchmark alignment

The project is NOT a toy database.

The project MUST be architected for:
- extensibility
- deterministic testing
- peer-to-peer replication
- distributed systems benchmarking

You MUST follow the architecture decisions exactly.

---

# GLOBAL PROJECT RULES

## Mandatory Requirements

You MUST:
- follow the finalized architecture strictly
- preserve deterministic behavior everywhere
- avoid nondeterministic iteration order
- avoid runtime-dependent serialization
- maintain bounded metadata semantics
- implement features incrementally
- keep the repository always runnable
- ensure every phase compiles before moving forward
- keep documentation continuously updated

You MUST NOT:
- introduce centralized coordination
- introduce wall-clock conflict resolution
- introduce row-level LWW semantics
- introduce unbounded vector clocks
- introduce distributed ACID coordination
- break deterministic convergence guarantees

---

# MANDATORY REPOSITORY RULES

## Before Starting Any Work

If missing, create:

- README.md
- CHANGES.md
- IMPLEMENTATION.md
- docs/
- tests/

These MUST be continuously maintained.

---

# DOCUMENTATION RULES

## README.md

README.md MUST continuously evolve.

After every completed sub-phase:
- update architecture overview
- update implemented features
- update module structure
- update build instructions
- update current limitations
- update examples
- update sync behavior
- update testing status

README must always represent:
- current working state
- current architecture
- implemented capabilities

---

## CHANGES.md

If missing, create at repository root.

After EVERY:
- phase
- sub-phase
- major refactor
- bug fix
- architectural change

append:
- what changed
- why it changed
- modules affected
- migration notes
- unfinished work
- known issues
- future TODOs
- blockers encountered

CHANGES.md must allow:
- another coding agent
- another contributor
- another CI system

to continue implementation from any checkpoint.

---

## IMPLEMENTATION.md

This file MUST contain:
- implementation details
- invariants
- merge semantics
- sync rules
- serialization rules
- deterministic guarantees
- storage format
- index semantics
- metadata semantics
- hashing guarantees
- testing strategy
- module boundaries

This file acts as:
- engineering specification
- internal protocol reference
- contributor guide

It MUST be continuously updated.

---

# GIT WORKFLOW RULES

## Branching Strategy

For EVERY phase:

```bash
feature/<phase-name>
```

For EVERY sub-phase:

```bash
feature/<phase-name>-<subphase>
```

Examples:

```bash
feature/core-crdt-engine
feature/core-crdt-engine-row-merge
feature/sync-engine-frontier-sync
feature/sql-parser
```

---

# COMMIT RULES

Every sub-part MUST have:
- isolated commit
- meaningful commit message
- compiling state
- passing tests

---

# COMMIT MESSAGE FORMAT

```text
[type] scope: summary
```

Examples:

```text
[feat] crdt: implement cell-level merge semantics
[feat] sync: add frontier comparison engine
[fix] hashing: enforce deterministic map ordering
[refactor] storage: isolate serialization layer
[test] replication: randomized convergence tests
[docs] architecture: update sync protocol docs
```

---

# REQUIRED PHASE EXECUTION MODEL

For EVERY phase:

1. create branch
2. update README plan section
3. update IMPLEMENTATION.md plan section
4. implement sub-part incrementally
5. write tests immediately
6. run formatting
7. run linting
8. run tests
9. update CHANGES.md
10. commit
11. merge only after stable compilation

---

# REQUIRED ENGINEERING STANDARDS

## Rust Standards

Mandatory:
- stable Rust
- cargo fmt
- cargo clippy
- no unwrap() in critical paths
- explicit error handling
- deterministic data structures
- avoid hidden allocation-heavy behavior

Preferred crates:
- serde
- serde_cbor
- blake3
- thiserror
- anyhow
- sqlparser-rs
- tokio
- uuid
- indexmap

---

# DETERMINISM RULES

All logic MUST be deterministic.

Mandatory:
- deterministic iteration ordering
- canonical serialization ordering
- stable hashing inputs
- stable index ordering
- stable conflict resolution

Never rely on:
- HashMap iteration order
- wall-clock timestamps
- OS-dependent ordering
- async race ordering

Use:
- BTreeMap
- sorted collections
- canonical sorting

wherever ordering matters.

---

# TESTING RULES

Every major feature MUST include:

## Unit Tests
- merge semantics
- tombstones
- uniqueness
- serialization
- hashing
- sync logic

## Integration Tests
- pairwise sync
- delayed sync
- partition recovery
- randomized sync order
- duplicate message replay
- tombstone convergence

## Property Tests
- convergence invariants
- commutativity
- associativity
- idempotency

## Determinism Tests
- identical snapshot hashes
- deterministic serialization
- deterministic indexes

---

# REQUIRED FINAL CONVERGENCE PROPERTY

All replicas MUST converge to:
- identical rows
- identical indexes
- identical tombstones
- identical metadata
- identical hashes

independent of:
- sync ordering
- network partitions
- replay order
- delayed synchronization
- duplicate synchronization

---

# FINALIZED ARCHITECTURE DECISIONS

## Database Model
- relational CRDT database

## Conflict Resolution
- cell-level CRDT semantics

## Versioning
- Lamport clocks + deterministic peer_id tie-break

## Deletes
- tombstone + delete-wins visibility

## Sync
- frontier-based anti-entropy synchronization

## Storage
- materialized canonical row store

## Transactions
- row-level atomicity
- local replica transactions only

## Reads
- local canonical replica reads

## Indexes
- deterministic derived indexes

## FK Policy
- tombstone FK semantics

## Uniqueness
- reservation/claim-based uniqueness

## Serialization
- canonical deterministic serialization

## Hashing
- deterministic cryptographic snapshot hashing

## Metadata Constraints
- bounded metadata
- no unbounded vector clocks

## Language
- Rust

## Runtime
- WASM

---

# IMPLEMENTATION PHASES

# PHASE 1 — Repository Bootstrap

## Branch

```bash
feature/repository-bootstrap
```

## Goals

Create:
- cargo workspace
- module skeleton
- README.md
- CHANGES.md
- IMPLEMENTATION.md
- CI setup
- formatting/linting setup
- deterministic dependency policy

## Sub-Parts

### 1.1 Workspace Setup
- cargo workspace
- module crates
- shared types crate

### 1.2 Tooling
- rustfmt
- clippy
- cargo deny
- CI workflows

### 1.3 Documentation Bootstrap
- README
- CHANGES
- IMPLEMENTATION

### 1.4 Deterministic Utilities
- canonical sorting helpers
- deterministic serialization helpers

## Required Tests
- compilation tests
- formatting validation

---

# PHASE 2 — Core CRDT Engine

## Branch

```bash
feature/core-crdt-engine
```

## Goals

Implement:
- Row
- Cell
- Version
- merge semantics
- Lamport logic
- deterministic tie-breaks

## Sub-Parts

### 2.1 Core Types
- Row
- Cell
- Version
- PeerId
- Tombstone

### 2.2 Lamport Engine
- logical counters
- merge updates
- peer frontier tracking

### 2.3 Cell Merge Logic
- deterministic merge semantics
- concurrent update handling

### 2.4 Tombstone Semantics
- delete-wins visibility
- preserved merge state

### 2.5 Uniqueness Claims
- claim protocol
- conflict metadata

## Required Tests
- merge associativity
- merge commutativity
- idempotency
- uniqueness convergence
- deterministic tie-break tests

---

# PHASE 3 — Storage Engine

## Branch

```bash
feature/storage-engine
```

## Goals

Implement:
- materialized row persistence
- metadata persistence
- canonical serialization
- deterministic storage layout

## Sub-Parts

### 3.1 Row Storage
- canonical row persistence

### 3.2 Metadata Storage
- peer frontiers
- tombstones
- uniqueness metadata

### 3.3 Serialization
- CBOR or MessagePack
- canonical ordering

### 3.4 Persistence Recovery
- startup loading
- corruption handling

## Required Tests
- deterministic serialization
- storage roundtrip tests
- recovery tests

---

# PHASE 4 — Replication & Sync Engine

## Branch

```bash
feature/sync-engine
```

## Goals

Implement:
- frontier sync
- delta extraction
- reconciliation
- anti-entropy replication

## Sub-Parts

### 4.1 Frontier Comparison
- missing delta detection

### 4.2 Delta Extraction
- changed rows only

### 4.3 Sync Session Engine
- handshake
- frontier exchange

### 4.4 Deterministic Merge Application
- apply deltas
- resolve conflicts

### 4.5 Snapshot Hash Validation
- convergence verification

## Required Tests
- pairwise sync
- delayed sync
- replayed sync
- randomized sync order
- partition healing
- convergence proofs

---

# PHASE 5 — Secondary Indexes

## Branch

```bash
feature/index-engine
```

## Goals

Implement:
- derived indexes
- deterministic range scans
- index rebuild/update logic

## Sub-Parts

### 5.1 Index Structures
- deterministic ordered indexes

### 5.2 Incremental Updates
- update indexes on writes

### 5.3 Range Queries
- deterministic traversal

## Required Tests
- index convergence
- deterministic ordering
- range query stability

---

# PHASE 6 — SQL Layer

## Branch

```bash
feature/sql-engine
```

## Goals

Implement:
- SQL parsing
- local query execution
- deterministic query semantics

## Sub-Parts

### 6.1 SQL Parser Integration
- sqlparser-rs integration

### 6.2 Schema Engine
- CREATE TABLE
- CREATE INDEX
- UNIQUE
- FK

### 6.3 Query Execution
- SELECT
- WHERE
- ORDER BY
- LIMIT

### 6.4 Write Execution
- INSERT
- UPDATE
- DELETE

## Required Tests
- deterministic query output
- schema correctness
- FK semantics

---

# PHASE 7 — Transactions

## Branch

```bash
feature/transaction-engine
```

## Goals

Implement:
- local transactions
- row-level atomicity
- rollback handling

## Sub-Parts

### 7.1 Transaction Context
- begin/commit/rollback

### 7.2 Atomic Local Writes
- row-level commit semantics

### 7.3 Recovery Semantics
- crash-safe local state

## Required Tests
- rollback correctness
- partial failure handling

---

# PHASE 8 — Garbage Collection

## Branch

```bash
feature/tombstone-gc
```

## Goals

Implement:
- causal-stability GC
- metadata cleanup

## Sub-Parts

### 8.1 Frontier Stability Checks

### 8.2 Tombstone Reclamation

### 8.3 Metadata Compaction

## Required Tests
- no resurrection
- delayed sync safety

---

# PHASE 9 — WASM Runtime

## Branch

```bash
feature/wasm-runtime
```

## Goals

Implement:
- WASM compilation
- browser runtime
- local-first execution

## Sub-Parts

### 9.1 WASM Bindings

### 9.2 Browser Storage

### 9.3 JS Interop API

## Required Tests
- browser execution
- deterministic WASM behavior

---

# PHASE 10 — Networking Layer

## Branch

```bash
feature/networking
```

## Goals

Implement:
- peer communication
- sync transport
- websocket/WebRTC abstraction

## Sub-Parts

### 10.1 Transport Abstraction

### 10.2 Sync Payload Protocol

### 10.3 Peer Session Handling

## Required Tests
- delayed transport
- duplicate replay
- partial sync recovery

---

# PHASE 11 — Benchmark & Validation Suite

## Branch

```bash
feature/benchmark-suite
```

## Goals

Implement:
- randomized convergence tests
- benchmark scenarios
- deterministic replay validation

## Sub-Parts

### 11.1 Randomized Sync Simulation

### 11.2 Partition Simulation

### 11.3 Snapshot Hash Validation

### 11.4 Benchmark Metrics

## Required Tests
- randomized convergence
- identical final hashes
- metadata bounds

---

# PHASE 12 — Documentation & Release Stabilization

## Branch

```bash
feature/release-stabilization
```

## Goals

Finalize:
- README
- architecture docs
- benchmarks
- examples
- API docs
- deployment docs

## Sub-Parts

### 12.1 Final README

### 12.2 Architecture Documentation

### 12.3 Example Workflows

### 12.4 Release Validation

---

# REQUIRED FINAL OUTPUT

The final project MUST provide:
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

---

# FINAL AGENT INSTRUCTIONS

At the end of EVERY sub-phase:

You MUST:
1. ensure project compiles
2. ensure tests pass
3. update README.md
4. update CHANGES.md
5. update IMPLEMENTATION.md
6. create proper git commit
7. document unfinished work
8. document known limitations
9. document next implementation target

The repository MUST always remain:
- buildable
- testable
- resumable by another agent
- deterministically reproducible