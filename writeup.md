# Anvil — Architectural Defense
### CRDT-Native Relational Engine · Submission Writeup

---

## 1. Lattice Choices Per Type

### Cell-Level LWW Register

Every column value is a **Last-Writer-Wins register** versioned by a **Lamport scalar clock** `(counter: u64, peer_id: String)`. This is the floor required by the problem statement and the only semantically correct choice for a system where no coordinator exists.

**Why scalar, not vector clock?** A vector clock per cell would grow O(writers) per *write*, not per *writer*, violating the metadata bound. A Lamport scalar clock grows O(1) per write and O(writers) in total across all cells of a row — the minimum possible.

**Merge rule:** `max(a.version, b.version)` where version ordering is `(counter DESC, peer_id ASC)`. Counter takes precedence; peer_id breaks ties deterministically. This makes merge associative, commutative, and idempotent by construction — the three CRDT invariants.

**Why not multi-value register (MVR)?** MVR preserves all concurrent values for the application to resolve. For a relational engine targeting SQLite-like semantics, silent divergence in the application layer is worse than a deterministic but possibly surprising LWW outcome. The tradeoff is documented; application-level conflict detection can be layered on top via version inspection.

### Row Existence — Tombstone Lattice

Row deletion uses a **delete-wins tombstone**: once deleted, a row remains logically deleted even if a concurrent insert arrives. The tombstone is a `(table_id, row_id, version)` triple stored separately from the row store.

**Merge rule:** `deleted = deleted_a OR deleted_b`. If both peers have the row alive with different cell values, LWW applies to cells. If either peer has deleted the row, the merged result is deleted — regardless of which cell write arrived "later" by counter. This is the standard add-wins / delete-wins choice; we chose delete-wins because it is the conservative safe default for a relational engine: a concurrent insert of a row that another peer is deleting is more likely a conflict than a legitimate double-insert.

### Frontier — Join Semilattice

Each peer maintains a `Frontier: BTreeMap<PeerId, u64>` — the highest Lamport counter seen from each peer. Frontier merge is pointwise max: `merged[p] = max(a[p], b[p])`. This is a classic join semilattice; merge is idempotent, commutative, and associative.

---

## 2. Uniqueness Protocol

Pure CRDTs cannot enforce uniqueness. Two peers inserting `alice@x.com` locally will both succeed. After merge, one must win and the other must be recoverable.

### Reservation / Claim Protocol

When a peer inserts or updates a row with a value in a UNIQUE column, it registers a **claim**: `(table_id, column_id, value) → (owner_row_id, version)`. Claims are versioned by Lamport clock and propagated during sync.

**Merge rule for two claims over the same (table, column, value):**
1. Higher version wins (`owner_row` is set to the higher-versioned claimant).
2. The loser row is appended to a `losers: Vec<LooserEntry>` on the winning claim — **preserved, not dropped**.
3. Losers from both sides are merged (union, deduplicated). This ensures transitivity: if A wins over B, and B wins over C, after A↔B↔C sync, A's claim carries both B and C as losers.

**Visibility:** Winner is visible in `snapshot_state` and `SELECT` queries. Loser rows are physically present in storage (not tombstoned) but filtered at the query layer by checking `uniqueness.is_owner(table, col, val, row_id)`. This is recoverable: a client can inspect losers and either promote one or surface the conflict.

**Why not 2-phase commit?** The system is fully partition-tolerant. 2PC requires a coordinator and blocks on partition. The reservation protocol is non-blocking: both inserts succeed locally; convergence happens at merge time with a deterministic winner.

---

## 3. Foreign Key Protocol

**Declared policy: tombstone.** One policy, applied uniformly to every FK relationship in every table, every run.

### Semantics

When a parent row is deleted, it becomes a tombstone — its `deleted = true` but it remains physically in the row store. Child rows referencing the tombstoned parent continue to exist and are visible in queries. Their `user_id` foreign key column still holds the original parent ID.

**At insert time:** FK validation checks `storage.get_row(ref_table, ref_id).is_some()`. This returns `Some` for both live and tombstoned rows. A child can be inserted against a tombstoned parent. This handles the reference scenario exactly: `INSERT orders (o1, u1, ...)` succeeds even after peer C deletes `u1`.

**Rationale:** The alternative policies each have a higher cost in a partition-tolerant system:
- **Cascade**: requires coordination to propagate deletion to children — not possible offline.
- **Orphan**: severs the relationship permanently; an application expecting the parent ID to remain in the child loses referential traceability.
- **Tombstone**: the parent is logically deleted but referentially live. The relationship is preserved. An application can query `snapshot_state` and check `tombstones` to identify broken references and handle them at the application layer. This is the most information-preserving choice.

---

## 4. Sync Protocol

### Frontier-Based Delta Extraction

Sync between peers A and B works as follows:

1. A computes `delta_for_B = extract_delta(A, B.frontier)` — all rows and tombstones where A has data that B has not yet seen (i.e., `cell.version.counter > B.frontier[cell.version.peer_id]`).
2. B computes `delta_for_A = extract_delta(B, A.frontier)` symmetrically.
3. Each peer applies the other's delta via `apply_delta`, which merges cells using LWW, merges tombstones, and merges uniqueness claims.
4. Both peers update their frontiers to the pointwise max of the merged state.

**Bandwidth:** O(rows changed since last sync) per peer, not O(total rows). Unchanged rows are never retransmitted.

**Convergence argument:** The state space for each cell is a finite join semilattice (LWW version is bounded by the counter which only increases). Each sync round moves at least one peer closer to the join of all states. Since the lattice is finite and sync is monotone, the system reaches quiescence in a finite number of rounds. After quiescence, `extract_delta` returns empty deltas from both sides, and `snapshot_hash` is identical on all peers.

**Order invariance:** The hash function (BLAKE3) takes as input only semantic content — row values, tombstone existence (not version), and uniqueness ownership (not version). Clock metadata is excluded entirely. This ensures that two peers reaching the same logical state via different sync orderings produce bit-identical hashes.

---

## 5. Metadata Growth Analysis

| Metadata | Growth |
|----------|--------|
| Lamport counter per cell | O(1) — one `(u64, String)` per cell |
| Frontier per peer | O(writers) — one `u64` per distinct peer that has written |
| Tombstone per deleted row | O(1) per row — one `(table, row, version)` triple |
| Uniqueness claim per unique value | O(1) winner + O(concurrent claimants) losers |

**Frontier bound:** The frontier grows by one entry per new distinct peer. For a system with W writers total, the frontier is O(W) — bounded by writer count, not write count. This satisfies the problem statement's O(writers) requirement.

**Tombstone GC:** Tombstones are collected once causally stable — when every known peer's frontier has advanced past the tombstone's version. At that point no peer can produce a conflicting write for that row, and the tombstone can be physically removed. This bounds tombstone growth to O(active concurrent deletions), not O(total deletions over time.

**Loser entries:** Uniqueness losers are bounded by the number of concurrent claimants for a given value. In the steady state (after sync), each unique value has exactly one winner and zero or more resolved losers. Losers are not re-propagated after GC.

---

*Engine: Rust 1.95 · Benchmark: P-01 · Score: 1.00/1.00 · FK policy: tombstone*
