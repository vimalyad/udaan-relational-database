# design_choices.md

# CRDT Relational Database — Finalized Architecture Decisions

---

# 1. Database Model

- Relational database with CRDT-based replication

## Design Reasons

- preserves familiar SQL relational abstractions
- supports tables, indexes, joins, and foreign keys
- enables local-first/offline-first behavior
- avoids centralized coordination during normal writes
- eventual convergence across disconnected peers
- benchmark-compatible relational semantics

---

# 2. Conflict Resolution Granularity

- Cell-Level CRDT semantics

## Reason

- preserves concurrent updates to different columns
- benchmark explicitly discourages row-level LWW
- avoids unnecessary overwrite of unrelated fields
- enables deterministic fine-grained merges
- minimizes logical write conflicts

## Example

Peer A:
```text
UPDATE users SET name='Alice'
```

Peer B:
```text
UPDATE users SET email='alice@x.com'
```

Merged result preserves BOTH updates.

---

# 3. Replication Model

- Rows/cells are mergeable CRDT states

## Reason

- peers stay disconnected until synchronization
- no causal broadcasts exist
- no realtime op propagation
- easier convergence reasoning
- avoids dependency ordering issues of pure op-based CRDTs
- anti-entropy synchronization maps naturally to state reconciliation
- robust under unreliable pairwise synchronization

---

# 4. Sync Architecture

- Frontier-based deterministic anti-entropy synchronization

## Sync Exchanges

Only:
- changed rows
- changed cells
- changed tombstones
- changed metadata

NOT:
- entire database state

## Sync Model

```text
sync(peerA, peerB)
```

pairwise bidirectional reconciliation.

## Sync Flow

### Step 1
Exchange:
- peer_id
- frontier metadata
- optional snapshot hash

### Step 2
Determine missing/outdated state using frontier comparison.

### Step 3
Transfer only missing deltas.

### Step 4
Apply deterministic CRDT merges.

### Step 5
Rebuild deterministic derived indexes.

### Step 6
Update synchronization frontier metadata.

---

## Sync-to-Quiescence Semantics

Repeated synchronization between peers MUST eventually reach quiescence.

Quiescence means:
- no new deltas exchanged
- identical frontier metadata
- identical snapshot hashes
- no further merge changes possible

Repeated sync passes MUST be:
- deterministic
- replay-safe
- idempotent

The engine MUST support:
- arbitrary repeated sync calls
- arbitrary sync ordering
- delayed synchronization
- duplicate synchronization sessions

---

## Design Reasons

- minimizes bandwidth usage
- scalable incremental replication
- deterministic convergence independent of sync order
- replay-safe synchronization
- naturally compatible with disconnected peers
- aligns with Dynamo/CouchDB anti-entropy concepts
- synchronization complexity approximately:

```text
O(changed_rows)
```

instead of:

```text
O(total_database_size)
```

---

# 5. Peer Model

- Fully disconnected peers before synchronization

## Properties

- local-first writes
- no central server
- no shared ordering
- no global coordinator
- writes never blocked on network availability

## Design Reasons

- enables offline-first operation
- avoids availability bottlenecks
- supports peer-to-peer deployments
- replicas may temporarily diverge
- deterministic convergence after synchronization
- low-latency local writes

---

# 6. DELETE vs UPDATE Conflict Semantics

- Tombstone + Delete-Wins visibility semantics

## DELETE Operation

DELETE:
- does NOT physically remove the row
- marks row as tombstoned

## Internal Merge Behavior

Concurrent updates:
- still merge normally
- row remains physically preserved internally

## Visibility Semantics

```text
deleted=true
```

dominates visibility.

Normal queries:
- exclude tombstoned rows by default

## Result

- concurrent updates preserved internally
- row remains logically deleted
- prevents unintended resurrection during synchronization

## Design Reasons

- preserves causal history
- deterministic convergence
- anti-entropy friendly
- avoids zombie rows
- safe delayed synchronization
- enables future tombstone garbage collection

---

# 7. Foreign Key Conflict Policy

- Tombstone FK semantics

## Behavior

- child rows survive parent deletion
- parent remains as tombstoned logical entity
- FK references remain intact
- referential linkage preserved internally

## Example

Peer A:
```text
DELETE user u1
```

Peer B:
```text
INSERT order o1 referencing u1
```

After merge:
- u1 exists as tombstoned row
- o1 survives
- FK reference preserved

## Query Semantics

Normal queries:
- hide tombstoned parents

Internal metadata:
- preserves FK linkage

## Design Reasons

- preserves concurrent inserts
- avoids destructive cascade data loss
- deterministic across peers
- naturally compatible with tombstone semantics
- avoids orphaned references
- benchmark-aligned FK behavior

---

# 8. Uniqueness Constraint Semantics

- Reservation / Claim-based uniqueness protocol

## Goal

Enforce deterministic uniqueness across disconnected peers without permanent coordination.

## Behavior

Unique values:
- represented as distributed ownership claims

Concurrent claims:
- merged deterministically

Winner selection:
- Lamport clock ordering
- deterministic peer_id tie-break

## Conflict Resolution

Only one row becomes canonical owner.

Losing rows:
- are NOT silently deleted
- remain internally preserved

---

## Conflict Visibility

Losing rows MUST:
- remain internally preserved
- remain queryable through debugging/introspection APIs
- retain causal metadata
- retain deterministic conflict markers

Normal queries MAY hide conflicted rows by default.

---

## Example

Peer A:
```text
users(u1, email='alice@x.com')
```

Peer B:
```text
users(u2, email='alice@x.com')
```

After merge:
- one canonical owner
- losing row preserved with conflict metadata

## Design Reasons

- pure CRDTs cannot guarantee uniqueness alone
- deterministic convergence
- preserves causal history
- satisfies benchmark recoverability requirement
- avoids silent data loss
- bounded metadata

---

# 9. Versioning / Causality Tracking

- Lamport logical clocks + deterministic peer_id tie-break

## Version Structure

```rust
(counter, peer_id)
```

## Clock Behavior

Each peer:
- maintains monotonically increasing logical counter
- increments counter on local writes
- updates local counters during synchronization

## Conflict Resolution

Higher Lamport counter wins.

If counters tie:
- deterministic peer_id ordering resolves conflict

## Example

```text
(17, peerA)
(17, peerB)
```

If:
```text
peerB > peerA
```

then:
```text
(17, peerB)
```

wins deterministically.

## Design Reasons

- deterministic convergence
- avoids unsafe wall-clock timestamps
- bounded metadata
- simpler than vector clocks
- scalable under increasing peer count
- synchronization-order independent merges

---

# 10. Secondary Index Architecture

- Deterministic derived indexes

## Index Model

Indexes are:
- derived state
- materialized from canonical merged rows

Base table rows remain:
- authoritative source of truth

## Structure

```rust
BTreeMap<IndexKey, BTreeSet<RowId>>
```

## Behavior

Indexes:
- rebuilt or incrementally updated deterministically
- exclude tombstoned rows
- converge automatically from canonical rows

## Query Semantics

Range queries:
- operate on deterministic merged index state

All replicas generate:
- identical index ordering

## Design Reasons

- prevents index drift/divergence
- deterministic convergence
- easier correctness reasoning
- indexes become pure functions of merged state

---

# 11. Tombstone Garbage Collection (GC)

- Causal-stability-based tombstone GC

## Goal

Safely reclaim tombstones without risking resurrection.

## Behavior

Tombstones retained until:
- all known peers observed delete

## GC Condition

For tombstone:
```text
(17, peerA)
```

GC allowed only if all known peer frontiers satisfy:

```text
frontier >= (17, peerA)
```

## Example Frontier

```rust
{
  peerA: 40,
  peerB: 28,
  peerC: 19
}
```

## Result

- no accidental resurrection
- bounded metadata growth
- preserved convergence guarantees

## Design Reasons

- prevents premature deletion
- anti-entropy correctness
- deterministic convergence
- long-running replication safety

---

# 12. Snapshot Hashing / Deterministic State Representation

- Canonical deterministic serialization + cryptographic hashing

## Goal

Guarantee:
- bit-identical snapshot hashes
- independent of sync order
- independent of merge history

---

## Canonicalization Rules

- tables sorted deterministically
- rows sorted by primary key ascending
- columns sorted lexicographically
- indexes sorted deterministically
- tombstones serialized deterministically
- metadata serialized deterministically

---

## Serialization

Canonical:
- CBOR
OR
- canonical MessagePack

---

## Hashing

Recommended:
- BLAKE3

---

## Included in Snapshot State

- merged rows
- tombstones
- uniqueness metadata
- FK metadata
- indexes
- frontier metadata

---

## Excluded

- runtime memory layout
- insertion order
- local cache ordering
- synchronization history artifacts

---

# snapshot_state Semantics

snapshot_state() MUST:
- return tables in deterministic order
- return rows sorted by primary key ascending
- return columns in deterministic order
- exclude tombstoned rows from normal visibility
- preserve FK tombstone semantics internally
- exclude runtime-only metadata
- exclude insertion-order artifacts

Snapshot representation MUST be:
- canonical
- deterministic
- stable across machines

---

## Design Reasons

- deterministic convergence verification
- benchmark-compatible hashing
- reproducible synchronization
- debugging simplicity

---

# 13. Physical Storage Architecture

- Materialized canonical row store

## Storage Model

Merged canonical rows persisted directly.

Storage acts as:
- local persistence engine

CRDT semantics remain:
- database-layer responsibility

## Physical Layout

```text
tables/
indexes/
tombstones/
uniqueness_claims/
peer_frontiers/
metadata/
```

## Row Representation

Rows store:
- cell values
- Lamport versions
- tombstone metadata
- uniqueness metadata

## Example

```rust
{
  cells: {
    email: {
      value: "alice@x.com",
      version: [17, "peerA"]
    }
  },
  deleted: false
}
```

## Design Reasons

- efficient reads
- efficient hashing
- easier debugging
- deterministic snapshots
- anti-entropy compatibility

---

# 14. Transaction / Atomicity Model

- Row-level atomicity + local replica transactions

## Guarantees

- local transaction atomicity
- row-level atomic writes
- deterministic eventual convergence

## Non-Guarantees

- no distributed ACID
- no global serializable isolation
- temporary cross-peer divergence possible

## Design Reasons

- offline-first compatibility
- avoids distributed coordination
- preserves availability
- AP-oriented architecture

---

# 15. Query Consistency Semantics

- Local canonical replica read semantics

## Query Model

Queries execute entirely against:
- local materialized replica

Reads NEVER require:
- network coordination

## Guarantees

- deterministic local reads
- eventual global convergence
- stable snapshot-style query execution

## Non-Guarantees

- no linearizable reads
- no globally synchronized reads

## Design Reasons

- offline-first support
- low-latency queries
- partition tolerance
- deterministic local execution

---

# 16. SQL / Query Engine Scope

- Minimal deterministic SQL subset

## Supported Schema Features

- CREATE TABLE
- PRIMARY KEY
- UNIQUE
- FOREIGN KEY
- CREATE INDEX

## Supported Writes

- INSERT
- UPDATE
- DELETE

## Supported Reads

- SELECT
- WHERE
- ORDER BY
- LIMIT

## Optional

- simple INNER JOIN

## Unsupported Initially

- recursive queries
- distributed joins
- triggers
- stored procedures
- analytical SQL
- distributed serializable transactions

## Parser Strategy

Recommended:
- sqlparser-rs

## Design Reasons

- focuses effort on replicated semantics
- deterministic execution
- benchmark-aligned scope
- easier correctness reasoning

---

# 17. Internal Module Architecture

## Core Module Layout

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
├── adapter/
└── tests/
```

---

## Core Data Structures

### Row

```rust
struct Row {
    id: RowId,
    cells: BTreeMap<ColumnId, Cell>,
    deleted: bool,
}
```

### Cell

```rust
struct Cell {
    value: Value,
    version: Version,
}
```

### Version

```rust
struct Version {
    counter: u64,
    peer_id: PeerId,
}
```

### Frontier

```rust
type Frontier = BTreeMap<PeerId, u64>;
```

---

# 18. CRDT Merge Invariants

All merge operations MUST satisfy:

- Associativity
- Commutativity
- Idempotency

Meaning:

```text
merge(a, merge(b, c))
==
merge(merge(a, b), c)
```

```text
merge(a, b)
==
merge(b, a)
```

```text
merge(a, a)
==
a
```

These guarantees are REQUIRED for:
- replay-safe synchronization
- duplicate message safety
- randomized sync-order convergence
- anti-entropy correctness

---

# 19. Coordination Constraints

The system MUST NOT require:

- centralized lock services
- global transaction coordinators
- leader election
- synchronized clocks
- globally ordered logs
- consensus protocols for normal writes

Local writes MUST remain available during network partitions.

---

# 20. Technology Stack

## Language

- Rust

## Runtime

- WASM

## Design Reasons

### Rust

- memory safety
- deterministic systems programming
- high performance
- strong serialization ecosystem
- distributed systems suitability

### WASM

- browser/server portability
- deterministic execution
- embedded local-first deployment
- offline-first browser replication support

---

# 21. Python Adapter Compatibility

The engine MUST support benchmark adapter integration.

Required adapter methods:

```python
open_peer()
apply_schema()
execute()
sync()
snapshot_hash()
snapshot_state()
close()
```

Recommended bridge:
- pyo3
OR
- subprocess bridge

The adapter layer MUST preserve:
- deterministic serialization
- deterministic snapshot visibility
- deterministic synchronization behavior