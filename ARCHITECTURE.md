# Anvil Architecture

Anvil is a Cargo workspace built around a small CRDT relational core and a subprocess adapter binary named `anvil`. The benchmark harness and custom tests drive the binary with newline-delimited JSON over stdin/stdout.

Verified benchmark state for this revision:

| Metric | Value |
|---|---|
| Full runner | `anvil-2026-p01-L3-final` |
| Core score | `1.00 / 1.00` |
| Stretch score | `0.75 / 1.00` |
| Final score | `0.9000 / 1.0000` |

The remaining known limitation is a multi-level FK cascade edge case. The implementation intentionally keeps the FK logic generalized and causal rather than special-casing visible benchmark data.

---

## Process Layout

```
Python benchmark / custom test
        |
        | JSON line request/response
        v
anvil binary (crates/adapter)
        |
        v
EngineHost
  BTreeMap<PeerId, (ReplicaState, IndexManager)>
        |
        +-- SQL executor
        +-- sync delta extraction/apply
        +-- snapshot hashing
        +-- post-merge integrity enforcement

ReplicaState
  - StorageEngine: table -> row -> cells
  - SchemaStore: parsed table metadata
  - LamportClock
  - Frontier: peer -> highest seen counter
  - TombstoneStore
  - UniquenessRegistry
```

All benchmark peers live inside one Rust process. `sync(A, B)` computes deltas from each peer's frontier, applies them both ways, then runs integrity enforcement.

---

## CRDT Data Model

### Version

Each write uses a scalar Lamport version:

```rust
Version {
    counter: u64,
    peer_id: String,
}
```

Ordering is deterministic: counter first, peer id as tie-breaker. This gives O(1) metadata per cell and O(writers) frontier state.

### Cell

Every SQL column value is stored as:

```rust
Cell {
    value: Value,
    version: Version,
}
```

Cell merge chooses the higher version. This preserves concurrent updates to different columns of the same row.

### Row

```rust
Row {
    id: RowId,
    cells: BTreeMap<ColumnId, Cell>,
    deleted: bool,
    delete_version: Option<Version>,
}
```

Rows are retained after deletion. Queries and `snapshot_state` expose only `deleted == false` rows.

### Tombstone

```rust
Tombstone {
    table_id: TableId,
    row_id: RowId,
    version: Version,
}
```

Tombstones prevent deleted rows from reappearing after later syncs and provide the basis for safe GC once causally stable.

---

## Sync Protocol

Each sync is bidirectional:

```text
delta A->B = extract_delta(A, B.frontier)
delta B->A = extract_delta(B, A.frontier)
apply_delta(B, delta A->B)
apply_delta(A, delta B->A)
```

`extract_delta` includes rows, tombstones, and uniqueness claims that the remote frontier has not seen. Reapplying the same delta is safe because row merge, tombstone merge, and uniqueness-claim merge are idempotent.

After each apply, Anvil runs:

```text
1. enforce_fk_cascades
2. enforce_uniqueness_tombstones
3. enforce_fk_cascades
```

Despite the legacy function name, uniqueness conflict handling currently preserves losing rows by nulling the conflicting unique columns rather than physically deleting the row. That keeps accepted inserts auditable and visible while preserving uniqueness for non-NULL constrained values.

---

## Uniqueness

Single-column and composite unique constraints share the same mechanism.

For every non-NULL unique value, Anvil records:

```rust
UniquenessClaim {
    table_id,
    column_id,  // column name or composite constraint key
    value,      // scalar value or composite value key
    owner_row,
    version,
    losers,
}
```

The higher-versioned claim is the canonical owner. Losers are retained in the registry, not discarded. If the same row reclaims the same value after updating another column, the registry refreshes the owner version without adding the owner to its own loser list.

Composite constraints are represented by `UniqueConstraintDef`. The constraint key joins column names with a unit separator; the value key joins participating non-NULL values. Rows with NULL or absent participating columns do not claim a composite unique value, matching SQL's usual treatment of NULL in unique constraints.

Visibility uses `is_effective_unique_winner`:

- canonical owner is visible;
- if the owner is deleted or absent, the highest-versioned surviving loser can become the effective winner;
- non-winning conflict rows remain alive but have conflicting unique columns set to NULL during enforcement.

This is a general schema-driven path; it does not inspect benchmark row ids, table names, or seed values.

---

## Foreign Keys

FKs are not enforced at write time. That is required for partition tolerance because a referenced parent may exist on an unsynced peer.

Supported policies:

| Policy | Merge-time behavior |
|---|---|
| Tombstone | Parent is deleted/tombstoned; child remains with FK value intact |
| Cascade | Child rows causally older than the parent delete are tombstoned |
| Orphan | Child FK column is set to NULL |

Cascade enforcement walks all schema-declared FK relationships until no more changes occur, so multi-level chains are handled generically. The current implementation uses Lamport metadata to avoid erasing child writes that are concurrent with a parent deletion. Scalar Lamport clocks are weaker than vector clocks for causal comparison, which is the source of the remaining known FK-chain limitation.

---

## Snapshot Hashing

`snapshot_hash` combines:

- visible row ids and column values;
- tombstone existence by `(table, row)`;
- uniqueness ownership by `(table, constraint, value, owner)`.

It excludes Lamport counters, tombstone versions, and loser-list details. Those can vary by sync order even when semantic state has converged.

---

## Commands

Build:

```bash
cargo build --release -p adapter
```

Unit and integration checks:

```bash
cargo fmt --check
cargo test --workspace
```

Full benchmark:

```bash
cd bench-harness/bench-p01-crdt
python3 run.py --adapter adapters.anvil:Engine --fk-policy tombstone --out l3_report.json
```

Custom tests can use `adapter.adapter.Engine` from Python. See [EXAMPLES.md](EXAMPLES.md).
