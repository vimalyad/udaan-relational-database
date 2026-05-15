# Anvil — Architecture

## System Overview

Anvil is structured as a Cargo workspace of 15 focused crates. The external surface is the `anvil` binary — a JSON-RPC subprocess that any language can drive. Internally, each replica is fully independent: it writes locally, merges on sync, and never contacts a coordinator.

```
                    ┌─────────────────────────────────┐
                    │    Benchmark / Application       │
                    │   (Python, via adapter.py RPC)   │
                    └──────────────┬──────────────────┘
                                   │  newline-delimited JSON
                    ┌──────────────▼──────────────────┐
                    │         anvil binary             │
                    │    (crates/adapter/src/main.rs)  │
                    └──────────────┬──────────────────┘
                                   │
                    ┌──────────────▼──────────────────┐
                    │          EngineHost              │
                    │   BTreeMap<PeerId, ReplicaState> │
                    └──┬──────────┬──────────┬────────┘
                       │          │          │
               ┌───────▼──┐ ┌────▼────┐ ┌──▼──────┐
               │ SQL Layer│ │  Sync   │ │ Hashing │
               │ executor │ │ engine  │ │ BLAKE3  │
               └───────┬──┘ └────┬────┘ └─────────┘
                       │         │
               ┌───────▼─────────▼──────────────────┐
               │           ReplicaState              │
               │  ┌──────────┐  ┌─────────────────┐ │
               │  │ Storage  │  │  CRDT State     │ │
               │  │ Engine   │  │  ┌────────────┐ │ │
               │  │ (rows,   │  │  │ LamportClk │ │ │
               │  │  schema) │  │  │ Tombstones │ │ │
               │  └──────────┘  │  │ Uniqueness │ │ │
               │                │  │ Frontier   │ │ │
               │                │  └────────────┘ │ │
               │                └─────────────────┘ │
               └────────────────────────────────────┘
```

---

## Data Model

### Version — The Lamport Scalar Clock

```
Version {
    counter: u64,       // monotonically increasing per peer
    peer_id: String,    // stable identifier of the originating peer
}

Ordering: (counter DESC, peer_id ASC)
```

Every cell write advances the local clock by 1. On receiving a remote delta, the clock advances to `max(local, remote) + 1`. The `peer_id` tie-break makes the total order deterministic with no coordination. This is a **scalar Lamport clock**, not a vector clock — O(1) per cell, O(writers) total metadata.

### Cell — The Unit of CRDT State

```
Cell {
    value:   Value,    // Null | Integer(i64) | Text(String) | Blob(Vec<u8>)
    version: Version,
}
```

Every column value in every row carries its own independent version. Merge is applied per-cell, not per-row. Concurrent writes to different columns both survive.

### Row

```
Row {
    id:             RowId,                    // = primary key value as String
    cells:          BTreeMap<ColumnId, Cell>, // sorted for determinism
    deleted:        bool,
    delete_version: Option<Version>,
}
```

Rows are never physically removed during normal operation. Deletion stamps `deleted = true` with a `delete_version`. Only rows where `deleted = false` are visible to queries.

### Tombstone

```
Tombstone {
    table_id: TableId,
    row_id:   RowId,
    version:  Version,   // when the deletion happened
}
```

Stored separately from the row store. Used for GC eligibility checks and delta propagation.

### Frontier

```
Frontier = BTreeMap<PeerId, u64>
```

A compact summary: the highest counter seen from each peer. A cell is "new" to a peer if `cell.version.counter > frontier[cell.version.peer_id]`. Grows O(1) per new distinct peer, not per write.

### SyncDelta

```
SyncDelta {
    source_peer:       PeerId,
    rows:              Vec<RowDelta>,
    tombstones:        Vec<Tombstone>,
    uniqueness_claims: Vec<UniquenessClaim>,
    frontier:          Frontier,
}
```

The payload exchanged during sync. Contains only state not yet seen by the remote peer.

---

## Merge Invariants

All merge operations satisfy the three lattice properties, making Anvil a **state-based CRDT**:

```
Associativity:  merge(A, merge(B, C)) = merge(merge(A, B), C)
Commutativity:  merge(A, B) = merge(B, A)
Idempotency:    merge(A, A) = A
```

These hold because every merge decision reduces to a comparison of `Version` values under the total order `(counter DESC, peer_id ASC)`. There are no random or time-dependent choices.

### Cell Merge

```
merge_cell(a, b) → whichever has the higher Version
```

### Row Merge

```
merge_row(a, b):
  for each column: cells[col] = merge_cell(a.cells[col], b.cells[col])
  deleted = a.deleted OR b.deleted           // delete-wins
  delete_version = max(a.delete_version, b.delete_version)
```

Delete-wins means once a row is deleted on any peer, it stays deleted after merge — regardless of concurrent cell writes.

---

## Sync Protocol

### Step-by-Step

```
Peer A                                    Peer B
   │                                         │
   │── extract_delta(A, B.frontier) ────────►│
   │   (rows/tombstones A has, B hasn't)     │
   │                                         │── apply_delta(B, delta_A)
   │                                         │   merge cells, tombstones,
   │                                         │   uniqueness claims
   │◄── extract_delta(B, A.frontier) ────────│
   │   (rows/tombstones B has, A hasn't)     │
   │── apply_delta(A, delta_B)               │
   │                                         │
   │── advance_frontier(A, B.frontier) ─────►│
   │◄── advance_frontier(B, A.frontier) ─────│
   │                                         │
[A.hash == B.hash]                    [quiescent]
```

### Delta Extraction

`extract_delta(source, remote_frontier)` scans every cell in every row:

```
for each table, row, cell:
    if cell.version.counter > remote_frontier[cell.version.peer_id]:
        include row in delta
for each tombstone:
    if tombstone.version.counter > remote_frontier[tombstone.version.peer_id]:
        include tombstone in delta
```

Bandwidth is O(rows changed since last sync), not O(total rows).

### Convergence Proof (Informal)

The state space per cell is a join semilattice under the `Version` order. Each `apply_delta` call is monotone — it moves the local state up the lattice, never down. The lattice is finite (bounded by the counter which only increases). Therefore, repeated pairwise sync reaches a fixed point (quiescence) in a finite number of rounds. At quiescence, `extract_delta` returns empty on both sides, and `snapshot_hash` is identical on all peers.

---

## Uniqueness Protocol

Pure CRDTs cannot enforce uniqueness — it requires at least one coordination step. Anvil uses a **reservation/claim protocol** that is non-blocking under partition and convergent on sync.

### Claim Lifecycle

```
INSERT INTO users (id, email) VALUES ('u1', 'alice@x.com')
       │
       ▼
registry.claim("users", "email", "alice@x.com", "u1", version)
       │
       ├── No existing claim  →  u1 becomes owner immediately
       │
       ├── Existing claim, incoming version HIGHER
       │       →  challenger becomes new owner
       │          previous owner appended to losers[]
       │
       └── Existing claim, incoming version LOWER
               →  challenger appended to losers[]
                  incumbent owner keeps the slot
```

### Merge

When two registries sync:

```
for each (table, col, value) present in both:
    winner = claim with higher version
    losers = union(losers_A, losers_B) - {winner.owner_row}
```

Losers from both sides are merged and deduplicated. This ensures transitivity across multi-hop sync.

### Visibility

The query layer calls `uniqueness.is_owner(table, col, val, row_id)` before returning each row. Loser rows are physically present in storage (not tombstoned) but hidden from `snapshot_state` and `SELECT`. This preserves auditability and allows application-level conflict surfacing.

---

## Snapshot Hashing

### Algorithm

```
full_hash = BLAKE3(
    hash_tables(visible rows: id + column values only),
    hash_tombstones(table_id + row_id only, sorted),
    hash_uniqueness(table + col + value + owner_row_id only, sorted)
)
```

### Included vs. Excluded

| Included | Excluded |
|---|---|
| Row IDs of visible rows | Lamport counters |
| Column values of visible rows | Peer IDs in versions |
| Which rows are deleted (existence) | Tombstone versions |
| Which row owns each unique value | Uniqueness claim versions |
| | Loser lists |

**Why exclude versions?** Lamport counters vary with sync order — the same logical insert from peer A may arrive at peer B with a different counter depending on what peer B had seen previously. Including clock metadata would make the hash sync-order-dependent, breaking the order-invariance property. By hashing only semantic content, two peers that have received the same set of logical operations in any order produce identical hashes.

---

## SQL Layer

Anvil implements a SQL executor over the CRDT storage engine using `sqlparser-rs 0.54`.

### Supported Statements

| Statement | CRDT Behaviour |
|---|---|
| `CREATE TABLE` | Registers schema; no version |
| `CREATE INDEX` | Creates BTreeMap secondary index |
| `INSERT INTO … VALUES` | Assigns Lamport version to each cell; registers uniqueness claims |
| `UPDATE … SET … WHERE` | Per-cell versioned update; re-registers uniqueness claims for UNIQUE cols |
| `DELETE FROM … WHERE` | Stamps `deleted=true` + `delete_version`; inserts tombstone |
| `SELECT … WHERE … ORDER BY … LIMIT` | Operates on visible rows only; tombstoned and uniqueness-loser rows hidden |

### FK Tombstone Policy

```
Parent deleted on peer C    Child inserted on peer A
        │                           │
        ▼                           ▼
  parent row:                 FK check passes because
  deleted=true                storage.get_row() returns
  (tombstone)                 Some for tombstoned rows
        │                           │
        └─────── sync ──────────────┘
                    │
                    ▼
           Both rows visible:
           - parent: tombstoned (hidden from queries)
           - child:  alive, user_id = parent's id
```

One policy, applied uniformly. Cascade and orphan require coordination or information loss under partition; tombstone preserves maximum information.

---

## Python Adapter (JSON-RPC Protocol)

The `anvil` binary reads newline-delimited JSON from stdin and writes responses to stdout. One request → one response.

### Request / Response Format

```json
Request:  {"cmd": "Execute", "args": {"peer_id": "A", "sql": "INSERT ...", "params": []}}
Response: {"status": "ok", "result": null}
          {"status": "error", "message": "table not found: users"}
```

### Command Reference

| Command | Arguments | Returns |
|---|---|---|
| `OpenPeer` | `peer_id` | `null` |
| `ApplySchema` | `peer_id`, `stmts: [str]` | `null` |
| `Execute` | `peer_id`, `sql`, `params` | rows or `null` |
| `Sync` | `peer_a`, `peer_b` | `null` |
| `SnapshotHash` | `peer_id` | `"<64-char hex>"` |
| `SnapshotState` | `peer_id` | `{"table": [{"col": val}]}` |
| `Close` | `{}` | `null` |
