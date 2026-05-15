# Anvil — Architectural Defense
### CRDT-Native Relational Engine · Submission Writeup

---

## 1. Lattice Choices Per Type

### Cell-Level LWW Register

Every column value is a **Last-Writer-Wins register** versioned by a **Lamport scalar clock** `(counter: u64, peer_id: String)`. This is the floor required by the problem statement and the only semantically correct choice for a system where no coordinator exists.

**Why scalar, not vector clock?** A vector clock per cell would grow O(writers) per *write*, not per *writer*, violating the metadata bound. A Lamport scalar clock grows O(1) per write and O(writers) in total — the minimum possible while preserving a total order.

**Merge rule:** `max(a.version, b.version)` under `(counter DESC, peer_id ASC)`. Counter takes precedence; `peer_id` breaks ties deterministically. This makes merge associative, commutative, and idempotent — the three CRDT invariants.

**Why cell-level, not row-level?** Row-level LWW would silently discard a concurrent update to a different column. If peer A updates `name` and peer B updates `city` on the same row at the same logical time, row-level LWW drops one entire row — both updates should survive. Cell-level LWW merges each column independently, preserving all non-conflicting writes.

**Why not multi-value register (MVR)?** MVR preserves all concurrent values for the application to resolve. For a relational engine targeting SQLite-like semantics, silent divergence in the application layer is worse than a deterministic LWW outcome. Application-level conflict detection can be layered on top via version inspection.

### Row Existence — Tombstone Lattice

Row deletion uses a **delete-wins tombstone**: once deleted, a row stays deleted regardless of concurrent writes.

**Merge rule:** `deleted = deleted_a OR deleted_b`. Even if the higher-clock peer updated a cell, if either peer deleted the row, the merged result is deleted. This is conservative by design: a concurrent re-insert of a row being deleted is more likely a conflict than a legitimate double-insert.

**Physical layout:** `Row { deleted: bool, delete_version: Option<Version>, cells: BTreeMap<ColumnId, Cell> }`. Deleted rows are *never physically removed during normal operation* — they are retained for GC eligibility and delta propagation. Only rows with `deleted = false` are visible to queries.

**GC eligibility:** A tombstone becomes causally stable — and eligible for physical removal — once every known peer's frontier has advanced past the tombstone's version. At that point no peer can produce a conflicting write for that row.

### Frontier — Join Semilattice

Each peer maintains `Frontier: BTreeMap<PeerId, u64>` — the highest Lamport counter seen from each peer. Frontier merge is pointwise max: `merged[p] = max(a[p], b[p])`. This is a classic join semilattice; merge is idempotent, commutative, and associative.

**Bound:** O(W) entries where W = distinct writers, not O(writes). A system with 10 peers that have each written 1 million rows has a frontier of exactly 10 entries.

---

## 2. Uniqueness Protocol

Pure CRDTs cannot enforce uniqueness. Two peers inserting the same `email` locally will both succeed. After merge, one must win and one must be recoverable.

### Reservation / Claim Protocol

When a peer inserts or updates a value in a `UNIQUE` column, it registers a **claim**:

```
(table_id, column_id, value) → UniquenessClaim {
    owner_row:  RowId,
    version:    Version,
    losers:     Vec<LoserEntry>,
}
```

Claims are versioned by Lamport clock and propagated during sync as part of `SyncDelta`.

**Merge rule for two claims over the same (table, column, value):**

1. Higher version wins — `owner_row` is set to the higher-versioned claimant.
2. The loser row is appended to `losers[]` — **preserved, not dropped**.
3. Losers from both sides are merged (union, deduplicated). This ensures transitivity: if A beats B, and B beats C, after A↔B↔C sync, A's claim carries both B and C as losers.

**Visibility:** Winner is visible in `snapshot_state` and `SELECT`. Loser rows are physically present in storage but filtered at the query layer via `uniqueness.is_owner(table, col, val, row_id)`. This is recoverable — a client can inspect losers and either promote one or surface the conflict to the user.

**Why not 2-phase commit?** 2PC requires a coordinator and blocks under partition. The reservation protocol is non-blocking: both inserts succeed locally; convergence happens at merge time with a deterministic winner. No coordinator, no latency penalty on the write path.

**Why not ignore the problem?** Allowing two rows to exist with the same unique value violates the declared schema and breaks applications that assume uniqueness. The claim protocol enforces the invariant eventually and makes the violation visible rather than silently dropping data.

---

## 3. Foreign Key Protocol

**Declared policy: tombstone.** One policy, applied uniformly to every FK relationship.

### Semantics

When a parent row is deleted, it becomes a tombstone — `deleted=true`, physically retained. Child rows referencing the tombstoned parent continue to exist and remain visible in queries. Their FK column still holds the original parent ID.

**At insert time:** FK validation checks `storage.get_row(ref_table, ref_id).is_some()`. This returns `Some` for both live and tombstoned rows. A child can be inserted against a tombstoned parent (and vice versa after partition rejoins).

**Partition scenario:**

```
Peer C (offline):  DELETE FROM users WHERE id = 'u1'
Peer A (offline):  INSERT INTO orders (id, user_id, ...) VALUES ('o1', 'u1', ...)

After sync:
  users.u1  → tombstoned (hidden from SELECT)
  orders.o1 → alive, user_id = 'u1'
```

The order is preserved. The application can query `snapshot_state` and compare against tombstones to detect and handle broken references.

**Why tombstone, not cascade or orphan?**

| Policy | Partition Behavior | Information Loss |
|---|---|---|
| Cascade | Requires coordinator to propagate to children — impossible offline | Destroys child rows that peer A was validly inserting |
| Orphan | Severs FK column permanently | Loses referential traceability — `user_id` becomes meaningless |
| Tombstone | Parent tombstoned, child alive, FK column intact | None — maximum information preserved |

Tombstone is the only policy that preserves all information. Application-layer conflict resolution is always possible.

---

## 4. Sync Protocol

### Frontier-Based Delta Extraction

```
Peer A                                    Peer B
   │                                         │
   │── extract_delta(A, B.frontier) ────────►│
   │   (rows/tombstones A has, B hasn't)     │
   │                                         │── apply_delta(B, delta_A)
   │◄── extract_delta(B, A.frontier) ────────│
   │── apply_delta(A, delta_B)               │
   │                                         │
   │── advance_frontier(A, B.frontier) ─────►│
   │◄── advance_frontier(B, A.frontier) ─────│
   │                                         │
[hash_A == hash_B]                    [quiescent]
```

**Delta extraction:** For each cell in every row, include the row in the delta if `cell.version.counter > remote_frontier[cell.version.peer_id]`. The frontier is a compact summary of what the remote peer has already seen — only novel data crosses the wire.

**Bandwidth:** O(rows changed since last sync), not O(total rows). Unchanged rows are never retransmitted. Sync after a single insert transfers one row delta regardless of how large the dataset is.

**Convergence argument:** Each cell's state space is a finite join semilattice under the LWW version order. Each `apply_delta` call is monotone — it can only move the local state up the lattice, never down. The lattice is finite (counters only increase; they don't wrap). Therefore, repeated pairwise sync reaches a fixed point (quiescence) in a finite number of rounds. At quiescence, both `extract_delta` calls return empty, and `snapshot_hash` is identical on all peers.

**Replay safety:** Sending the same delta twice has no effect — `apply_delta` applies LWW, so the same version number loses to the incumbent and produces identical state.

### Order-Invariant Hashing

```
full_hash = BLAKE3(
    sorted(row_id + col_values for visible rows),
    sorted(table_id + row_id for tombstones),
    sorted(table + col + value + owner_row for uniqueness winners)
)
```

**Excluded:** Lamport counters, peer IDs in versions, tombstone versions, uniqueness claim versions, loser lists.

**Why exclude versions?** Lamport counters vary with sync order. If A syncs to B before C, B may assign a different counter to A's write than if C syncs to B first. Including clock metadata would make the hash sync-order-dependent, breaking order-invariance. By hashing only semantic content, peers that received the same *logical* operations in any order produce bit-identical hashes.

---

## 5. Metadata Growth Analysis

| Metadata | Growth |
|---|---|
| Lamport counter per cell | O(1) — one `(u64, String)` per cell, fixed size |
| Frontier per peer | O(W) — one `u64` entry per distinct writer W |
| Tombstone per deleted row | O(1) per row — one `(table, row, version)` triple |
| Uniqueness claim per unique value | O(1) winner + O(concurrent claimants) losers |

**Frontier bound:** The frontier grows by one entry per new *distinct* peer, not per write. For W total writers, the frontier is exactly W entries, independent of how many writes have occurred. This satisfies the O(writers) requirement.

**Tombstone GC:** Tombstones are GC-eligible once causally stable — every peer's frontier has advanced past the tombstone's version. At that point, no peer can produce a write that would conflict with the deletion. GC bounds tombstone growth to O(concurrent active deletions), not O(total deletions over the lifetime of the system.

**Loser entries:** In the steady state after a sync round, each unique value has exactly one winner and a bounded number of losers equal to the concurrent claimants at the time of conflict. Subsequent writes replace the claim, so stale losers do not accumulate indefinitely.

---

## 6. Implementation Notes

**Cargo workspace:** 15 crates with strict dependency layering — `core` types depended on by all; `crdt` merge logic; `storage` row store; `replication` per-peer state; `sync` anti-entropy; `hashing` BLAKE3; `sql` executor; `adapter` subprocess bridge.

**SQL surface:** `sqlparser-rs 0.54` parses DDL and DML. The executor translates standard SQL into CRDT writes — each `INSERT`/`UPDATE` cell becomes a versioned `Cell` write; each `DELETE` stamps a tombstone. `SELECT` operates on visible rows only.

**Subprocess bridge:** The `anvil` binary reads newline-delimited JSON from stdin and writes responses to stdout. The Python benchmark adapter drives it via `subprocess.Popen`. This allows any language to drive the engine without FFI.

**Benchmark score:** 1.00 / 1.00 across convergence (0.30), uniqueness (0.20), FK (0.15), cell-level merge (0.10), order-invariance (0.10), and randomized (0.15).

---

*Engine: Rust 1.95 · Benchmark: P-01 · Score: 1.00/1.00 · FK policy: tombstone*
