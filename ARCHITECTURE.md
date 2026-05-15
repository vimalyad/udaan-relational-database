# Anvil Architecture

## Data Model

### Version

```
Version { counter: u64, peer_id: String }
```

A Lamport logical timestamp paired with a stable peer identifier. Ordering: higher `counter` wins. On equal counters, the lexicographically higher `peer_id` wins. This makes ordering total and deterministic with no coordination.

### Cell

```
Cell { value: Value, version: Version }
```

The smallest unit of CRDT state. Every column value in every row carries its own independent version. Values are typed: `Null`, `Integer(i64)`, `Text(String)`, or `Blob(Vec<u8>)`.

### Row

```
Row { id: RowId, cells: BTreeMap<ColumnId, Cell>, deleted: bool, delete_version: Option<Version> }
```

A row is a map from column name to `Cell`. Deletion is recorded in-place via `deleted = true` and `delete_version`; the row is never physically removed from storage until GC runs. Only rows with `deleted = false` are visible to queries (`is_visible()`).

### Frontier

```
Frontier = BTreeMap<PeerId, u64>
```

A compact summary of the highest Lamport counter observed from each peer. Used as the unit of anti-entropy comparison: if a cell's `version.counter > frontier[version.peer_id]`, it is new to that peer.

### SyncDelta

```
SyncDelta {
    source_peer: PeerId,
    rows: Vec<RowDelta>,
    tombstones: Vec<Tombstone>,
    uniqueness_claims: Vec<UniquenessClaim>,
    frontier: Frontier,
}
```

The payload exchanged between peers during sync. Contains only state newer than what the remote peer has already seen, plus the source peer's full frontier to advance the remote clock.

---

## Merge Invariants

All merge operations on cells, rows, tombstones, and uniqueness claims satisfy the three CRDT lattice properties:

**Associativity**

```
merge(a, merge(b, c)) == merge(merge(a, b), c)
```

Applying deltas in any grouping produces the same final state. This means multi-hop relay (peer A → peer B → peer C) is equivalent to direct exchange.

**Commutativity**

```
merge(a, b) == merge(b, a)
```

Applying a delta from peer A on top of peer B's state, or vice versa, yields the same result. Sync order does not matter.

**Idempotency**

```
merge(a, a) == a
```

Replaying the same delta has no effect. Retransmission and duplicate delivery are safe without deduplication logic.

These properties hold because every merge decision reduces to a deterministic comparison of `Version` values using the total order `(counter, peer_id)`.

---

## Sync Protocol

Sync between two peers is a bidirectional frontier-based delta exchange.

### Frontier Comparison

Each peer tracks `Frontier: BTreeMap<PeerId, u64>`. Before sending, peer A asks: for each row and tombstone, is `version.counter > remote_frontier[version.peer_id]`? If yes, the remote peer has not yet seen that version.

### Delta Extraction (`extract_delta`)

`extract_delta(source, remote_frontier)` scans all rows in all tables. A row is included in the delta if any of its cells, or its `delete_version`, has a counter greater than what is recorded in `remote_frontier` for that peer. Tombstones are filtered the same way. Uniqueness claims are always included in full (they are small and idempotent to re-apply).

### Delta Application (`apply_delta`)

`apply_delta(target, delta)` performs:

1. For each `RowDelta`: merge the incoming row with the local row using cell-level LWW. If no local row exists, insert it directly.
2. For each tombstone: update the in-storage row's `deleted` and `delete_version` if the incoming version is higher than the locally recorded one; insert the tombstone into the local tombstone store.
3. Merge uniqueness claims from the delta into the local `UniquenessRegistry` (higher version wins).
4. Advance the local Lamport clock from the remote frontier.
5. Merge the remote frontier into the local frontier; update the local peer's own entry.

### Bidirectional Sync

A full pairwise sync between peers A and B consists of:
1. `apply_delta(B, extract_delta(A, B.frontier))`
2. `apply_delta(A, extract_delta(B, A.frontier))`

After one round, both peers converge to the same logical state.

---

## Uniqueness Protocol

### Overview

UNIQUE constraints are implemented as a convergent reservation/claim protocol in `UniquenessRegistry`. Rather than rejecting conflicting inserts, all rows survive and the protocol deterministically selects one canonical owner.

### Claim Semantics

When a row inserts or updates a UNIQUE column, it calls:

```
registry.claim(table_id, column_id, value, row_id, version)
```

The registry stores `(table, column, value) -> UniquenessClaim { owner_row, version, losers }`.

- If no claim exists for the key, the row becomes the owner immediately.
- If a claim exists and the incoming version is higher, the challenger becomes the new owner. The previous owner is appended to `losers`.
- If the incoming version is lower (or equal with a different row), the challenger is appended to `losers`. The incumbent owner keeps the slot.

### Loser Preservation

Losers are never deleted. Their rows remain in storage; they are simply marked as non-owners of the unique value. This preserves auditability and allows the application to detect conflicts after the fact. Queries that enforce UNIQUE semantics filter out loser rows from visible results.

### Merge

`UniquenessRegistry::merge` applies the same higher-version-wins rule across two registries. The losers list from both sides is merged and deduplicated by `row_id`.

---

## Snapshot Hashing

### Algorithm

`SnapshotHasher` (in the `hashing` crate) computes a deterministic BLAKE3 hash over the canonical database state using three sub-hashes combined in sequence:

1. **Table hash** — iterates tables in BTree (alphabetical) order, rows in primary-key order, columns in lexicographic order, hashing column values only.
2. **Tombstone hash** — iterates tombstones sorted by `(table_id, row_id)`, hashing only the pair (not the version).
3. **Uniqueness hash** — iterates claims sorted by `(table_id, column_id, value)`, hashing only the owner `row_id` (not the version or losers list).

The three sub-hashes are fed into a final BLAKE3 hasher to produce the full snapshot hash.

### What is Hashed

- Row IDs of visible rows
- Column values of visible rows
- Existence of tombstones (which rows are deleted)
- Ownership assignment for unique values (which row owns each unique value)

### What is NOT Hashed

- Lamport versions / counters
- Peer IDs embedded in versions
- Tombstone versions
- Uniqueness claim versions
- Loser lists

### Why

Lamport clocks vary with sync order — the same logical insert from peer A may arrive at peer B with a different counter depending on when the sync happened. Excluding all clock metadata means the hash is **order-invariant**: two peers that have received the same set of operations in any order will produce the same snapshot hash, which is the convergence check used by the benchmark validation suite.

---

## SQL Layer

### Supported Statements

| Statement | Notes |
|---|---|
| `CREATE TABLE` | Parses column types, `PRIMARY KEY`, `NOT NULL`, `UNIQUE`, `DEFAULT`, `REFERENCES ... ON DELETE` |
| `CREATE INDEX` | Creates a named secondary index over one or more columns |
| `INSERT INTO ... VALUES` | Assigns a Lamport version to every inserted cell; registers uniqueness claims |
| `UPDATE ... SET ... WHERE` | Per-cell versioned update; re-registers uniqueness claims for updated UNIQUE columns |
| `DELETE FROM ... WHERE` | Stamps matched rows with `deleted = true` and a delete version; inserts tombstones |
| `SELECT ... FROM ... WHERE ... ORDER BY ... LIMIT` | Operates on visible rows only (tombstoned rows hidden); supports `*` and named column projections |

SQL is parsed by `sqlparser-rs`. Parameterized queries are supported via positional `?` placeholders.

### FK Tombstone Policy

Foreign key references use the `Tombstone` policy. When a parent row is deleted:

- The parent row is marked as a tombstone in storage.
- Any child row that references it via a FK column continues to exist and remain visible.
- The tombstoned parent row is hidden from normal queries but its row ID remains resolvable for FK integrity checks.
- This policy is declared in the schema as `ON DELETE CASCADE` mapped to `FkPolicy::Tombstone` at the engine level.

---

## Python Adapter

### Overview

The Python adapter (`adapter/adapter.py`) is a thin bridge between the Python benchmark harness and the Rust engine. It communicates with the `anvil` subprocess binary via a newline-delimited JSON-RPC protocol over stdin/stdout.

### Protocol

Each request is a JSON object followed by a newline:

```json
{"cmd": "<Command>", "args": { ... }}
```

Each response is a JSON object followed by a newline:

```json
{"status": "ok", "result": <value>}
{"status": "error", "message": "<description>"}
```

### Commands

| Command | Arguments | Returns |
|---|---|---|
| `OpenPeer` | `peer_id: str` | null |
| `ApplySchema` | `peer_id: str, stmts: [str]` | null |
| `Execute` | `peer_id: str, sql: str, params: [value]` | query result rows or null |
| `Sync` | `peer_a: str, peer_b: str` | null |
| `SnapshotHash` | `peer_id: str` | hex string |
| `SnapshotState` | `peer_id: str` | `{table: [{col: value}]}` |
| `Close` | `{}` | null |

### Engine Class

The `Engine` class in `adapter.py` implements the benchmark harness adapter interface. It locates the `anvil` binary (preferring the release build), spawns it as a subprocess, and proxies all method calls through `_AnvilProcess`. The `close()` method sends a `Close` command and waits for the process to exit cleanly.
