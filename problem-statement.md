# Anvil — A CRDT-Native Relational Engine

> A database that never disagrees, even when nobody is online.

Build an embeddable OLTP engine whose external surface looks like SQLite — tables, secondary indexes, joins, foreign-key cascades, uniqueness — and whose internal representation is CRDT all the way down. Multiple writers mutate locally without coordination. Merges converge. Invariants survive.

---

## Motivation

Local-first applications, multi-region deployments, and offline-tolerant systems all break against the same wall. CRDTs have solved blobs, registers, sequences, and sets. The relational primitives developers actually use — secondary indexes under concurrent inserts, foreign-key cascades under partition, range queries with deterministic order, uniqueness constraints — collapse the moment two writers diverge.

Replicache, ElectricSQL, PowerSync, and Y-sweet each ship a partial answer. The full relational story remains open. It is a real systems problem with a real commercial wedge.

---

## Challenge

Identify which relational invariants are natively CRDT-expressible, which require lattice-respecting encoding, and which fundamentally require a coordination protocol. For each category, defend a chosen semantics and prove convergence — informally but rigorously.

| Category | Examples |
|---|---|
| Natively CRDT-expressible | Set membership, last-writer-wins fields, monotonic counters |
| Lattice-respecting encoding required | Multi-value registers for conflicting cell writes, observed-remove sets for membership under concurrent insert/delete |
| Coordination protocol required | Uniqueness, escrow-style constraints, foreign-key existence under deletion |

---

## Required Capabilities

### SQL Surface

- `CREATE TABLE` with a primary key and at least one secondary index
- `INSERT` / `UPDATE` / `DELETE` over N concurrent peers without a coordinator

### Deterministic Reads

Range queries that return a deterministically merged result regardless of the order in which peer updates were applied.

### Foreign Keys

Cascade behavior under partition with a chosen, documented, defended semantics — not silence.

### Uniqueness

At least one uniqueness constraint enforced via an explicit escrow or reservation protocol. Pure CRDTs cannot do this. Address it directly.

### Sync

A pairwise sync protocol with bounded metadata and a written argument for eventual convergence.

---

## Stretch Vectors

- **Bitemporal** — Transaction-time and valid-time queries over the merged history
- **Causal Reads** — Monotonic-read and read-your-writes guarantees at the client
- **Schema Migration** — Online schema change under partition — the unsolved final boss

---

## Anti-Patterns · Auto-Disqualifying

### Server-Authoritative + Optimistic UI

This is the problem being solved, not the solution. Submissions that fall back to a single source of truth on conflict will be rejected.

### Row-Level LWW

Last-writer-wins on the entire row is degenerate and fails on any non-trivial schema. Cell-level resolution is the floor.

### Unbounded Metadata

Vector clocks or version vectors that grow without bound in writer count. Garbage collection is part of the protocol, not an afterthought.

---

## Deliverable

A working engine — Rust + WASM, TypeScript + IndexedDB, or any language with strong type discipline.

**Demonstrate:**

- Two laptops offline
- Both `INSERT` rows
- Both `UPDATE` different columns of the same row
- One `DELETE`s a parent row while the other `INSERT`s a child
- Reconnect over an unreliable network

**Show:**

- A deterministic merged state on both peers
- A transcript of how each invariant was preserved
- A five-page architectural defense of every coordination decision made

> The interesting code is not in the SQL parser. It is in the moments where the lattice breaks and you have to pick — and own — a coordination protocol.

---

## Annex A — Operational Contract

*This section is binding. Where the manifesto above is open to interpretation, the annex is not. If your design disagrees with the annex, the annex wins.*

---

### Interface · Surface

The engine ships as either an **embedded library** OR a **local-process daemon** speaking a documented wire protocol. Pick one and stay with it.

#### Embedded

```python
db = open("./mydb", peer_id="A")
db.execute("CREATE TABLE ...")
db.execute("INSERT ...")
db.sync_with(peer_b)            # pairwise, bidirectional
state_hash = db.snapshot_hash()
```

#### Daemon

```
POST /sql        { "stmt": "INSERT ...", "params": [...] }
POST /sync       { "peer": "B", "since": "<cursor>" }
GET  /snapshot   -> { "hash": "...", "tables": {...} }
```

---

### Reference Schema

All teams test against the same schema. Custom schemas are permitted in writeup discussion but not in the judged scenario.

```sql
CREATE TABLE users (
  id    TEXT PRIMARY KEY,
  email TEXT NOT NULL UNIQUE,
  name  TEXT
);

CREATE TABLE orders (
  id          TEXT PRIMARY KEY,
  user_id     TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  status      TEXT NOT NULL,
  total_cents INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX orders_by_user ON orders(user_id, status);
```

---

### Reference Scenario

Three peers **A · B · C**. All start empty and disconnected. Operations execute locally in the order shown.

**Sync semantics** — `Sync(a, b)` is symmetric and bidirectional. Either argument may initiate; on return both peers reflect each other's known state per the engine's merge rules. Peer state changes only via local execution or sync; there is no broadcast.

#### Trace

```
A: INSERT users (u1, alice@x.com, "Alice")
A: INSERT users (u2, bob@x.com,   "Bob")
B: INSERT users (u3, alice@x.com, "Alice'")   # uniqueness conflict on email
C: (after one-way sync from A — holds u1, u2)
C: DELETE users WHERE id = u1                 # parent delete under partition
A: INSERT orders (o1, u1, "pending", 1200)    # child insert vs deleted parent
A: UPDATE users SET name  = 'Alice Cooper'    WHERE id = u1
B: UPDATE users SET email = 'alice@ex.org'    WHERE id = u1   # cell-level conflict

# Pairwise sync A↔B ↔ B↔C ↔ A↔C until quiescent.
```

#### Required Assertions

Run by judges:

1. **Bit-identical merged state** across A, B, C — verified by snapshot hash
2. **Uniqueness invariant** on `users.email` holds. The winning row is documented and the loser is recoverable (not silently dropped)
3. **Foreign-key behavior** for `o1` against the deleted `u1` is documented and consistent with your stated semantics (kept-with-tombstone / cascaded / promoted-to-orphan — your choice, but pick one and defend it)
4. **Cell-level merge** on `users.u1` preserves both updates (`name` and `email`) without whole-row LWW
5. **Convergence is invariant under sync ordering** — a randomized re-run produces the same final hash

---

### Metadata Bound

Sync metadata per row must be bounded by **O(writers)**, where `writers` is the count of distinct peers that have written that row. Version vectors / vector clocks that grow per write are auto-disqualifying. Garbage collection is part of the protocol, not an afterthought.

---

### FK Policy · Declaration

Foreign-key behaviour under partition is not a per-row choice. Your engine declares **one policy** as a first-class property, applied uniformly across every FK conflict, every table, every run.

| Policy | Behaviour | Cost |
|---|---|---|
| `cascade` | The child row vanishes with the parent. Concurrent child writes that referenced the deleted parent are lost — you must defend this trade-off. | Lowest |
| `tombstone` | The child row survives. Its `user_id` column references a tombstoned row preserved in metadata. Queries see the child; the engine documents that the parent is logically deleted but referentially live. | Medium |
| `orphan` | The child row survives with `user_id` set to `NULL` or a documented sentinel. The relationship is severed; the child stands alone. | Medium |

You declare the policy to the benchmark via `--fk-policy {cascade|tombstone|orphan}`. The harness asserts observed state matches the declaration on the `(o1, u1)` conflict in the reference scenario. Switching policy mid-run, or picking different policies for different tables, is disqualifying — **one principled choice, defended in the writeup**.

---

### Dependencies

#### Permitted

Existing CRDT primitives (Automerge, Yjs, Loro, Diamond Types, Y-CRDT) used as register / sequence / set building blocks. The relational layer — indexes, joins, FK, uniqueness, query — must be your own work.

#### Forbidden

Using a server-side relational engine (Postgres / MySQL / SQLite as authoritative store) as the merged source of truth. SQLite as a local read-side cache is fine.

#### Required

Disclose every dependency in your `README`, including version pins.

---

### Constraints

| | |
|---|---|
| **Team size** | 1 – 4 |
| **Time** | 24 hours |
| **Language** | Open |
| **Hardware** | Commodity laptop · Linux or macOS |
| **Network** | Local-only; no cloud calls in the judged scenario |

---

## Submission

| Item | Requirement |
|---|---|
| **Repository** | Git link, public or private. Apache-2.0 / MIT preferred |
| **Quickstart** | README must produce a green run on a clean machine in under 5 minutes |
| **Reproducibility** | Dockerfile, Nix flake, or equivalent. The reference scenario must produce a bit-identical hash on judges' machines |
| **Demo** | 5-minute screen-recorded walkthrough of the reference scenario |
| **Writeup** | 3-page PDF defending: lattice choices per type, uniqueness protocol, FK protocol, sync protocol, metadata-growth analysis |

---

## Judging Mechanics

| Phase | Description |
|---|---|
| **Automated** | Judges run the reference scenario container. Check merged-state hash determinism and all invariants listed above |
| **Chaos** | A hidden variant reorders the operation trace with a fresh random seed. Final state must hash identically to the original run |
| **Manual** | Writeup graded on architectural defense, particularly the uniqueness and FK protocols |
| **Live** | 15-minute Q&A. Expect questions on convergence proofs and corner cases |

---

## Bench · Run It Yourself

The full evaluation harness is open and runs on your machine. Pure Python, stdlib only, no network access, no external dependencies. Five steps from clone to score.

### Step 1 · Clone

```shell
git clone https://github.com/Sauhard74/Anvil-P-E
cd Anvil-P-E/bench-p01-crdt
```

### Step 2 · Read the Interface

Open `adapter.py`. Seven methods every submission must implement: `open_peer`, `apply_schema`, `execute`, `sync`, `snapshot_hash`, `snapshot_state`, `close`.

### Step 3 · Write Your Adapter

Create `adapters/myteam.py` as a thin shim over your engine. For non-Python engines, bridge via subprocess, gRPC, or HTTP.

```python
# adapters/myteam.py
from adapter import Adapter

class Engine(Adapter):
    def __init__(self):
        self.peers = {}                          # peer_id -> your engine

    def open_peer(self, peer_id):
        self.peers[peer_id] = MyEngine.new()

    def apply_schema(self, peer_id, stmts):
        for s in stmts:
            self.peers[peer_id].execute(s)

    def execute(self, peer_id, sql, params=()):
        self.peers[peer_id].execute(sql, params)

    def sync(self, peer_a, peer_b):
        self.peers[peer_a].sync_with(self.peers[peer_b])

    def snapshot_hash(self, peer_id):
        return self.peers[peer_id].state_hash()

    def snapshot_state(self, peer_id):
        return self.peers[peer_id].dump_tables()

    def close(self):
        for p in self.peers.values():
            p.shutdown()
```

### Step 4 · Self-Check

Run the condensed battery. `--quick` uses fewer seeds for fast iteration.

```shell
python self_check.py --adapter adapters.myteam:Engine \
  --fk-policy tombstone --quick
```

You'll see a per-axis pass/fail matrix and a weighted score. Iterate until all axes pass.

### Step 5 · Full Run with Arbitrary Seeds

When the condensed run is green, drop `--quick` for the full battery. Stress-test with any L2 seeds — if your engine is correct, every seed passes.

```shell
python run.py --adapter adapters.myteam:Engine --fk-policy tombstone \
  --randomized-seeds 9999 31415 27182 16180 11235 \
  --rand-peers 5 --rand-ops 150 --out report.json
```

---

### Evaluation Layers

The bench resists hardcoding by design. Three layers of evaluation:

| Layer | Name | Description |
|---|---|---|
| **L1** | Canonical | The 3-peer trace described above. Passing is necessary but not sufficient |
| **L2** | Property-based | Pass `--randomized-seeds` any integers. Generator produces fresh ops per seed; assertions are pure CRDT invariants — convergence, uniqueness, idempotent sync. A hardcoded solution cannot pass because the expected hash is unknown until the run |
| **L3** | Adversarial | Held-out seeds and hand-crafted edge cases at higher parameter values. Used only at final evaluation. Not distributed |