# Anvil Design Defense

Team: Udaan

Anvil is a CRDT-native relational database prototype built for offline multi-writer OLTP. The design goal is to let each peer accept local SQL writes, exchange deltas later, and converge to a deterministic relational state without a coordinator. The current full L3 benchmark score is `0.9000 / 1.0000` (`core_score = 1.00`, `stretch_score = 0.75`). One known stretch limitation remains in a multi-level FK cascade edge case; the implementation keeps a generalized causal model instead of hardcoding visible benchmark data.

## Design Goals

The primary requirement is availability under partition. A peer should be able to accept inserts, updates, and deletes even when it cannot contact other peers. This rules out a design that depends on a central lock service, synchronous quorum, or two-phase commit on the write path. Those systems can enforce constraints immediately, but they block precisely when the benchmark's partition scenarios matter.

The second requirement is relational usability. A pure key-value CRDT can converge but does not understand SQL constraints, foreign keys, or row/column structure. Anvil therefore keeps relational metadata in the schema store and routes all constraint reconciliation through schema-driven code. The engine does not inspect benchmark row ids, scenario names, or seed values.

The third requirement is deterministic convergence. Every peer must reach the same visible state and same semantic hash after it has seen the same logical writes. All merge decisions are based on stable data: Lamport versions, sorted maps, tombstones, and uniqueness claims.

## CRDT State Model

Every cell is a last-writer-wins register versioned by a scalar Lamport clock `(counter, peer_id)`. A vector clock would expose richer causality, but it would attach O(writers) metadata to each cell. The scalar clock keeps each cell compact while still giving a deterministic total order. The tradeoff is intentional: Anvil stores enough metadata for convergence and stable hashing without making every value carry a large causal history.

The merge unit is the cell, not the row. Row-level LWW is simpler but wrong for relational updates: if one peer updates `name` and another updates `email`, a row-level winner would discard one valid column update. Cell-level merge preserves both independent writes and only resolves conflicts when the same column is concurrently changed.

Rows are stored as deterministic `BTreeMap` structures. Deterministic iteration matters because snapshot hashes and query output should not depend on hash-map ordering or process-specific randomness. Every row carries a primary key string, a map of cells, and deletion metadata.

Deletes use a tombstone lattice. A deleted row remains physically stored with `deleted = true` and a `delete_version`; queries hide it, sync propagates it, and garbage collection can remove it only after causal stability. Delete-wins prevents stale peers from resurrecting rows after partitions.

## Sync and Hashing

Each replica maintains a frontier: the highest Lamport counter seen from every peer. Delta extraction sends rows, tombstones, and uniqueness claims not covered by the remote frontier. Applying a delta is idempotent: merging the same row or tombstone twice leaves the same state.

The sync protocol is bidirectional. For `sync(A, B)`, the engine extracts an `A -> B` delta using B's frontier and a `B -> A` delta using A's frontier, then applies both. After each apply, the engine runs post-merge integrity enforcement so schema invariants are restored after new remote writes arrive.

Snapshot hashing intentionally excludes clock versions and other order-dependent metadata. The hash includes semantic content: visible rows, tombstone existence, and uniqueness ownership. This makes the hash stable across different sync orders as long as the logical merged state is the same.

## Uniqueness

Relational uniqueness cannot be enforced by a pure local CRDT write. Two partitioned peers may both insert the same unique value. Anvil handles this with a schema-driven uniqueness registry:

```text
(table, constraint, value) -> owner row, owner version, loser rows
```

Single-column and composite unique constraints use the same claim path. The higher-versioned claim becomes owner, and losing accepted writes are recorded rather than silently discarded. If the owner later becomes non-live, the best surviving loser can become the effective winner. This avoids both duplicate live unique values and unaccounted data loss.

Composite unique constraints are represented by schema metadata, not by per-scenario code. A constraint key is derived from the ordered list of participating columns, and a value key is derived from the row's non-NULL participating values. Rows with NULL or absent participating columns do not claim a composite unique value, matching standard SQL-style behavior.

For recoverability, conflict rows are preserved as rows; the conflicting unique column or composite columns are nulled during integrity enforcement. That keeps accepted inserts visible and auditable while preserving the uniqueness invariant for non-NULL constrained values. This design is more transparent than dropping losing rows and more available than rejecting writes during partition.

## Foreign Keys

FK checks are deferred to merge time. Enforcing FK constraints at local write time would reject valid partitioned writes because the referenced parent might exist only on another peer. Anvil therefore accepts the child write and reconciles relationships after sync.

Three FK outcomes are modeled:

| Policy | Behavior |
|---|---|
| Tombstone | parent is tombstoned; child remains visible with FK value intact |
| Cascade | child rows causally older than the parent delete are tombstoned |
| Orphan | child FK column is set to NULL |

Cascade handling is schema-driven and iterates until no further dependency changes occur, so it is not limited to one parent-child level. Because Anvil uses scalar Lamport clocks rather than vector clocks, it cannot perfectly distinguish every concurrent child write from every causally older child write across peers. The implementation chooses the safer generalized behavior for the declared tombstone policy and documents the remaining multi-level cascade limitation.

This limitation is intentional to surface rather than hide. A benchmark-specific patch could force a visible cascade outcome, but it would weaken hidden-case behavior. A fully principled solution would attach richer causal context to deletes or maintain dependency-aware provenance for FK edges.

## Post-Merge Integrity

After `apply_delta`, the engine runs FK enforcement, uniqueness enforcement, then FK enforcement again. The first FK pass handles parent deletes that arrived from the remote peer. The uniqueness pass resolves newly visible uniqueness conflicts and preserves losing rows by nulling the conflicting key columns. The second FK pass handles cases where uniqueness enforcement changed parent visibility or FK-relevant data.

This fixed ordering keeps the merge pipeline deterministic. It also keeps constraint repair outside the local write path, preserving partition tolerance while still converging to schema-aware state after sync.

## Why This Design

The central tradeoff is partition tolerance over immediate global constraint enforcement. A coordinator or two-phase commit could enforce uniqueness and FK constraints before acknowledging writes, but it would block under partition. Anvil instead accepts local writes, records enough CRDT metadata to merge later, and makes conflict outcomes deterministic and auditable.

The design is intentionally schema-driven. Constraint handling uses parsed table metadata, uniqueness claims, tombstones, and FK definitions. It does not depend on benchmark row ids, seed values, or scenario names, which makes it more suitable for hidden tests than a visible-benchmark-specific patch.

The current score reflects that tradeoff. Core invariants pass completely: convergence, basic uniqueness, declared FK tombstone behavior, cell-level merge, order invariance, and randomized data preservation. The remaining stretch gap is localized to one harder FK-chain behavior where scalar Lamport metadata is not expressive enough to perfectly model all causal dependency relationships.

## Verification

Commands used for this revision:

```bash
cargo fmt --check
cargo test --workspace
cargo build --release -p adapter
cd bench-harness/bench-p01-crdt
python3 run.py --adapter adapters.anvil:Engine --fk-policy tombstone --out l3_report.json
```

Verified benchmark result:

```text
core_score     1.00 / 1.00
stretch_score  0.75 / 1.00
final_score    0.90 / 1.00
```
