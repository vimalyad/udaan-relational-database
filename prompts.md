# PROMPTS.md

# CRDT Relational Database — Coding Agent Master Prompt

---

# SYSTEM ROLE

You are implementing a production-grade deterministic distributed relational database using:

- CRDT-based replication
- local-first/offline-first semantics
- pairwise anti-entropy synchronization
- deterministic convergence guarantees
- Rust + WASM architecture

The system MUST satisfy:
- benchmark correctness
- deterministic replay safety
- bounded metadata constraints
- sync-order-independent convergence
- randomized convergence validation

This is NOT a toy database.

The implementation MUST prioritize:

1. correctness
2. determinism
3. reproducibility
4. modularity
5. replay safety
6. benchmark alignment

---

# MANDATORY ARCHITECTURAL RULES

You MUST follow the architecture described in:
- design_choices.md
- IMPLEMENTATION.md

STRICTLY.

You MUST NOT:
- invent new consistency semantics
- introduce hidden coordination
- introduce nondeterministic behavior
- change merge semantics
- change uniqueness semantics
- change FK semantics
- change synchronization semantics

without explicitly documenting changes.

---

# CORE DATABASE GUARANTEES

The database MUST provide:

- deterministic convergence
- replay-safe synchronization
- bounded metadata growth
- disconnected local writes
- deterministic hashing
- deterministic indexes
- deterministic query results
- deterministic serialization

All replicas MUST converge to:
- identical rows
- identical indexes
- identical tombstones
- identical metadata
- identical snapshot hashes

independent of:
- synchronization order
- duplicate synchronization
- delayed synchronization
- network partitions
- replay ordering

---

# FORBIDDEN IMPLEMENTATION BEHAVIOR

NEVER:
- use wall-clock timestamps for conflict resolution
- use HashMap where ordering matters
- use runtime-dependent serialization
- use append-only operation histories
- use vector clocks with unbounded growth
- introduce distributed ACID transactions
- introduce centralized coordination
- introduce hidden leader election
- introduce global write ordering

---

# REQUIRED DATA STRUCTURES

Use deterministic collections ONLY.

Mandatory:
- BTreeMap
- BTreeSet

Forbidden:
- HashMap for deterministic state
- HashSet for deterministic state

---

# REQUIRED VERSIONING MODEL

Conflict resolution MUST use:

```rust
(counter, peer_id)
```

Rules:
- larger counter wins
- equal counters resolved using deterministic peer_id ordering

No wall-clock timestamps allowed.

---

# REQUIRED MERGE INVARIANTS

ALL merge operations MUST satisfy:

## Associativity

```text
merge(a, merge(b, c))
==
merge(merge(a, b), c)
```

## Commutativity

```text
merge(a, b)
==
merge(b, a)
```

## Idempotency

```text
merge(a, a)
==
a
```

These invariants are REQUIRED.

Breaking them is considered a critical correctness failure.

---

# REQUIRED SYNC MODEL

Synchronization MUST be:
- anti-entropy based
- pairwise
- replay-safe
- deterministic
- idempotent

Synchronization MUST support:
- arbitrary replay
- duplicate messages
- delayed synchronization
- arbitrary sync ordering

---

# REQUIRED SNAPSHOT SEMANTICS

snapshot_state() MUST:
- return deterministic table ordering
- return rows sorted by primary key ascending
- return deterministic column ordering
- exclude tombstoned rows from normal visibility
- preserve internal FK linkage
- exclude runtime-only metadata

snapshot_hash() MUST:
- be deterministic
- be stable across machines
- be independent of sync history

---

# REQUIRED UNIQUENESS SEMANTICS

Uniqueness MUST use:
- reservation/claim protocol

Conflicting rows MUST:
- remain internally preserved
- retain conflict metadata
- remain recoverable

No silent deletion allowed.

---

# REQUIRED FK SEMANTICS

Foreign keys MUST use:
- tombstone FK semantics

Parent deletion MUST NOT:
- destroy concurrent child inserts

Referential linkage MUST remain internally preserved.

---

# REQUIRED ENGINEERING STANDARDS

Mandatory:
- stable Rust
- cargo fmt
- cargo clippy
- explicit error handling
- deterministic serialization
- isolated commits
- phase-by-phase implementation

Forbidden:
- unwrap() in critical paths
- hidden nondeterminism
- runtime-dependent iteration ordering

---

# REQUIRED DOCUMENTATION RULES

The following files MUST always exist:

- README.md
- CHANGES.md
- IMPLEMENTATION.md
- design_choices.md

---

# README.md RULES

README.md MUST continuously evolve.

After EVERY completed sub-phase:
- update implemented features
- update architecture diagrams
- update usage examples
- update current limitations
- update build instructions
- update testing status

README.md MUST always represent:
- current repository state
- current architecture
- current implementation coverage

---

# CHANGES.md RULES

After EVERY:
- phase
- sub-phase
- refactor
- architectural change
- bug fix

append:
- what changed
- why it changed
- affected modules
- migration notes
- blockers
- unfinished work
- future TODOs

CHANGES.md MUST allow another coding agent to continue implementation from any checkpoint.

---

# IMPLEMENTATION.md RULES

IMPLEMENTATION.md MUST continuously document:
- invariants
- merge semantics
- sync semantics
- hashing rules
- serialization rules
- module responsibilities
- implementation details
- recovery semantics
- testing requirements

---

# REQUIRED GIT WORKFLOW

---

# BRANCHING RULES

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
feature/core-crdt-engine-merge
feature/storage-engine-serialization
feature/sync-engine-frontiers
```

---

# COMMIT RULES

Every sub-part MUST:
- compile
- pass tests
- update documentation
- use isolated commit

---

# COMMIT FORMAT

```text
[type] scope: summary
```

Examples:

```text
[feat] crdt: implement deterministic cell merge semantics
[feat] sync: implement frontier reconciliation
[fix] hashing: enforce canonical ordering
[test] replication: add randomized convergence tests
[docs] architecture: update synchronization semantics
```

---

# REQUIRED IMPLEMENTATION EXECUTION MODEL

For EVERY sub-phase:

1. create branch
2. update README planning section
3. update IMPLEMENTATION.md planning section
4. implement feature incrementally
5. immediately add tests
6. run cargo fmt
7. run cargo clippy
8. run tests
9. update CHANGES.md
10. create isolated commit
11. verify deterministic behavior

The repository MUST remain:
- buildable
- testable
- resumable
- replay-safe
- deterministic

at ALL times.

---

# REQUIRED TESTING STRATEGY

Every major feature MUST include:

## Unit Tests
- merge semantics
- Lamport ordering
- tombstones
- serialization
- hashing

---

## Integration Tests
- pairwise synchronization
- delayed synchronization
- duplicate synchronization
- partition recovery
- replay safety

---

## Property Tests
- associativity
- commutativity
- idempotency

---

## Determinism Tests
- deterministic hashes
- deterministic indexes
- deterministic serialization
- deterministic query results

---

# REQUIRED PHASE EXECUTION ORDER

The coding agent MUST implement phases IN ORDER.

DO NOT skip phases.

---

# PHASE 1 — Repository Bootstrap

Branch:

```bash
feature/repository-bootstrap
```

Required:
- cargo workspace
- crate structure
- CI setup
- deterministic utility layer
- base documentation

Required commits:

```text
[feat] bootstrap: initialize cargo workspace
[feat] tooling: configure CI and linting
[docs] bootstrap: initialize documentation
```

---

# PHASE 2 — Core CRDT Engine

Branch:

```bash
feature/core-crdt-engine
```

Required:
- Version
- Cell
- Row
- Lamport clocks
- merge semantics
- tombstones
- uniqueness claims

Required commits:

```text
[feat] crdt: implement core CRDT structures
[feat] crdt: implement Lamport clock engine
[feat] crdt: implement deterministic merge semantics
[feat] crdt: implement tombstone semantics
[feat] crdt: implement uniqueness claim protocol
```

---

# PHASE 3 — Storage Engine

Branch:

```bash
feature/storage-engine
```

Required:
- canonical persistence
- deterministic serialization
- metadata persistence
- recovery semantics

Required commits:

```text
[feat] storage: implement canonical persistence
[feat] storage: implement deterministic serialization
[feat] storage: implement recovery semantics
```

---

# PHASE 4 — Replication & Sync Engine

Branch:

```bash
feature/sync-engine
```

Required:
- frontier comparison
- delta extraction
- replay-safe synchronization
- deterministic reconciliation
- convergence hashing

Required commits:

```text
[feat] sync: implement frontier comparison
[feat] sync: implement delta extraction
[feat] sync: implement sync sessions
[feat] sync: implement deterministic reconciliation
[feat] hashing: implement convergence hashing
```

---

# PHASE 5 — Secondary Indexes

Branch:

```bash
feature/index-engine
```

Required:
- deterministic indexes
- incremental maintenance
- deterministic range scans

Required commits:

```text
[feat] index: implement deterministic indexes
[feat] index: implement incremental index maintenance
[feat] index: implement deterministic range scans
```

---

# PHASE 6 — SQL Engine

Branch:

```bash
feature/sql-engine
```

Required:
- sqlparser-rs integration
- schema engine
- deterministic query execution
- write execution

Required commits:

```text
[feat] sql: integrate sqlparser-rs
[feat] sql: implement schema engine
[feat] sql: implement deterministic query execution
[feat] sql: implement write execution
```

---

# PHASE 7 — Transactions

Branch:

```bash
feature/transaction-engine
```

Required:
- local transaction context
- row-level atomicity
- rollback semantics
- crash-safe recovery

Required commits:

```text
[feat] txn: implement transaction context
[feat] txn: implement atomic local writes
[feat] txn: implement recovery semantics
```

---

# PHASE 8 — Garbage Collection

Branch:

```bash
feature/tombstone-gc
```

Required:
- causal stability tracking
- tombstone reclamation
- metadata compaction

Required commits:

```text
[feat] gc: implement causal stability tracking
[feat] gc: implement tombstone reclamation
[feat] gc: implement metadata compaction
```

---

# PHASE 9 — WASM Runtime

Branch:

```bash
feature/wasm-runtime
```

Required:
- wasm-bindgen integration
- browser persistence
- JS interop APIs

Required commits:

```text
[feat] wasm: implement wasm bindings
[feat] wasm: implement browser persistence
[feat] wasm: implement JavaScript interop APIs
```

---

# PHASE 10 — Networking

Branch:

```bash
feature/networking
```

Required:
- transport abstraction
- sync transport
- peer sessions

Required commits:

```text
[feat] network: implement transport abstraction
[feat] network: implement sync transport
[feat] network: implement peer session handling
```

---

# PHASE 11 — Python Adapter

Branch:

```bash
feature/python-adapter
```

Required:
- benchmark adapter API
- Rust bridge layer
- deterministic snapshot bridge
- harness compatibility

Required commits:

```text
[feat] adapter: implement benchmark adapter interface
[feat] adapter: implement Rust bridge layer
[test] adapter: validate benchmark harness compatibility
```

---

# PHASE 12 — Benchmark Suite

Branch:

```bash
feature/benchmark-suite
```

Required:
- randomized synchronization testing
- partition simulation
- convergence verification
- benchmark metrics

Required commits:

```text
[test] benchmark: implement randomized synchronization tests
[test] benchmark: implement partition simulation
[test] benchmark: implement convergence validation
[feat] benchmark: implement benchmark metrics
```

---

# PHASE 13 — Release Stabilization

Branch:

```bash
feature/release-stabilization
```

Required:
- final documentation
- examples
- deployment instructions
- release validation

Required commits:

```text
[docs] release: finalize documentation
[docs] release: add deployment examples
[test] release: finalize validation suite
```

---

# FINAL SUCCESS CRITERIA

The final project MUST provide:

- deterministic CRDT relational database
- local-first execution
- offline-first replication
- anti-entropy synchronization
- deterministic convergence
- replay-safe synchronization
- bounded metadata
- deterministic hashing
- deterministic indexes
- deterministic relational queries
- Rust + WASM runtime
- benchmark-compatible adapter layer
- randomized convergence validation
- partition-tolerant synchronization
- hidden-chaos-test resilience

The final repository MUST be:
- buildable
- testable
- resumable
- reproducible
- benchmark-ready