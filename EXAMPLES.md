# Anvil — Workflow Examples

All examples use the Python adapter (`adapter/adapter.py`). Build the binary first:

```bash
cargo build --release -p adapter
```

---

## Example 1: Basic Insert and Sync Between Two Peers

Two peers start with no data. Each inserts a row locally. After a bidirectional sync, both peers converge to the same state and produce matching snapshot hashes.

```python
from adapter.adapter import Engine

e = Engine()

e.open_peer("A")
e.open_peer("B")

schema = [
    "CREATE TABLE users (id TEXT PRIMARY KEY, email TEXT NOT NULL, name TEXT)",
]
e.apply_schema("A", schema)
e.apply_schema("B", schema)

# Each peer inserts a different row while offline
e.execute("A", "INSERT INTO users (id, email, name) VALUES ('u1', 'alice@example.com', 'Alice')")
e.execute("B", "INSERT INTO users (id, email, name) VALUES ('u2', 'bob@example.com', 'Bob')")

# Bidirectional sync: A learns about u2, B learns about u1
e.sync("A", "B")

hash_a = e.snapshot_hash("A")
hash_b = e.snapshot_hash("B")
assert hash_a == hash_b, "peers must converge"

state = e.snapshot_state("A")
print(state["users"])
# [{'id': 'u1', 'email': 'alice@example.com', 'name': 'Alice'},
#  {'id': 'u2', 'email': 'bob@example.com', 'name': 'Bob'}]

e.close()
```

**What happens internally:** Each insert stamps a Lamport clock version on every cell. During sync, `extract_delta` sends only rows whose cell versions exceed the remote peer's frontier. Both peers converge to an identical snapshot; `snapshot_hash` confirms it with a BLAKE3 digest computed over semantic content only.

---

## Example 2: Cell-Level Merge — Concurrent Updates to Different Columns

Two peers concurrently update *different* columns of the same row. Both updates survive — this is the key advantage of cell-level over row-level LWW.

```python
from adapter.adapter import Engine

e = Engine()
e.open_peer("A")
e.open_peer("B")

schema = ["CREATE TABLE users (id TEXT PRIMARY KEY, email TEXT, name TEXT, city TEXT)"]
e.apply_schema("A", schema)
e.apply_schema("B", schema)

# Seed both peers with the same row
e.execute("A", "INSERT INTO users (id, email, name, city) VALUES ('u1', 'alice@x.com', 'Alice', 'NYC')")
e.sync("A", "B")

# Partition: A updates name, B updates city — different columns
e.execute("A", "UPDATE users SET name = 'Alice Smith' WHERE id = 'u1'")
e.execute("B", "UPDATE users SET city = 'London' WHERE id = 'u1'")

# Rejoin: both column updates survive
e.sync("A", "B")

state = e.snapshot_state("A")
row = state["users"][0]
assert row["name"] == "Alice Smith", "A's name update must survive"
assert row["city"] == "London",      "B's city update must survive"

hash_a = e.snapshot_hash("A")
hash_b = e.snapshot_hash("B")
assert hash_a == hash_b

print(f"name={row['name']} city={row['city']}")
# name=Alice Smith city=London

e.close()
```

**What happens internally:** Each column carries its own `Cell { value, version }`. Merge applies LWW per-cell, so the higher-versioned value wins for each column independently. Row-level LWW would have picked one peer's entire row, silently losing the other column update.

---

## Example 3: Concurrent Conflict — Same Column, LWW Resolution

Two peers update the *same* column of the same row. The higher Lamport clock wins.

```python
from adapter.adapter import Engine

e = Engine()
e.open_peer("A")
e.open_peer("B")

schema = ["CREATE TABLE users (id TEXT PRIMARY KEY, name TEXT)"]
e.apply_schema("A", schema)
e.apply_schema("B", schema)

e.execute("A", "INSERT INTO users (id, name) VALUES ('u1', 'Alice')")
e.sync("A", "B")

# Concurrent conflicting updates
e.execute("A", "UPDATE users SET name = 'Alice A' WHERE id = 'u1'")
e.execute("B", "UPDATE users SET name = 'Alice B' WHERE id = 'u1'")

e.sync("A", "B")

hash_a = e.snapshot_hash("A")
hash_b = e.snapshot_hash("B")
assert hash_a == hash_b, "both peers agree on the winner"

state = e.snapshot_state("A")
winner = state["users"][0]["name"]
print(f"Winner: {winner}")
# Both peers see the same name — whichever had the higher Lamport counter

e.close()
```

---

## Example 4: Uniqueness Conflict — Two Peers Claim the Same Email

Two peers insert rows with the same `UNIQUE` email while partitioned. After sync, one row becomes the owner; the other is preserved as a loser (filtered from normal queries).

```python
from adapter.adapter import Engine

e = Engine()
e.open_peer("A")
e.open_peer("B")

schema = [
    "CREATE TABLE users (id TEXT PRIMARY KEY, email TEXT NOT NULL UNIQUE, name TEXT)",
]
e.apply_schema("A", schema)
e.apply_schema("B", schema)

# Both peers insert with the same email while offline
e.execute("A", "INSERT INTO users (id, email, name) VALUES ('u1', 'shared@x.com', 'Alice')")
e.execute("B", "INSERT INTO users (id, email, name) VALUES ('u2', 'shared@x.com', 'Bob')")

e.sync("A", "B")

hash_a = e.snapshot_hash("A")
hash_b = e.snapshot_hash("B")
assert hash_a == hash_b, "peers converge"

state = e.snapshot_state("A")
owners = [r for r in state["users"] if r["email"] == "shared@x.com"]
assert len(owners) == 1, "exactly one row visible per unique value"

print(f"Owner row: {owners[0]['id']}")
# Either 'u1' or 'u2' — whichever had the higher Lamport clock

e.close()
```

**What happens internally:** The uniqueness registry stores `(table, column, value) → (owner_row_id, version, losers[])`. On sync, the higher-versioned claim wins. The loser row remains in storage (it is not tombstoned) but `is_owner()` returns false for it, so `snapshot_state` and `SELECT` filter it out.

---

## Example 5: Delete-Wins Tombstone — Concurrent Delete and Update

One peer deletes a row; the other concurrently updates it. Delete-wins semantics ensure the row stays deleted after sync.

```python
from adapter.adapter import Engine

e = Engine()
e.open_peer("A")
e.open_peer("B")

schema = ["CREATE TABLE users (id TEXT PRIMARY KEY, name TEXT)"]
e.apply_schema("A", schema)
e.apply_schema("B", schema)

e.execute("A", "INSERT INTO users (id, name) VALUES ('u1', 'Alice')")
e.sync("A", "B")

# Partition: A deletes, B updates
e.execute("A", "DELETE FROM users WHERE id = 'u1'")
e.execute("B", "UPDATE users SET name = 'Alice Updated' WHERE id = 'u1'")

e.sync("A", "B")

state = e.snapshot_state("A")
visible = state.get("users", [])
assert len(visible) == 0, "delete must win over concurrent update"

hash_a = e.snapshot_hash("A")
hash_b = e.snapshot_hash("B")
assert hash_a == hash_b

print("Row tombstoned on both peers. Hashes match.")

e.close()
```

**What happens internally:** Deletion stamps `deleted=true` and writes a `Tombstone { table_id, row_id, version }`. Row merge applies `deleted = deleted_a OR deleted_b`. The tombstone is included in the BLAKE3 hash by existence (not by version), so peers agree on which rows are deleted regardless of sync order.

---

## Example 6: Foreign Key Under Partition — Child Outlives Parent

A child row is inserted on one peer while the parent is deleted on another. After sync, the child survives (tombstone FK policy).

```python
from adapter.adapter import Engine

e = Engine()
e.open_peer("A")
e.open_peer("B")
e.open_peer("C")

schema = [
    "CREATE TABLE users  (id TEXT PRIMARY KEY, name TEXT)",
    "CREATE TABLE orders (id TEXT PRIMARY KEY, user_id TEXT REFERENCES users(id), item TEXT)",
]
e.apply_schema("A", schema)
e.apply_schema("B", schema)
e.apply_schema("C", schema)

# Seed user u1 on all peers
e.execute("A", "INSERT INTO users (id, name) VALUES ('u1', 'Alice')")
e.sync("A", "B")
e.sync("A", "C")

# Partition: C deletes u1; A inserts an order for u1
e.execute("C", "DELETE FROM users WHERE id = 'u1'")
e.execute("A", "INSERT INTO orders (id, user_id, item) VALUES ('o1', 'u1', 'Widget')")

# Rejoin
e.sync("A", "B")
e.sync("B", "C")
e.sync("A", "C")

state = e.snapshot_state("A")
orders = state.get("orders", [])
assert any(o["id"] == "o1" for o in orders), "child order must survive parent deletion"
users  = state.get("users",  [])
assert len(users) == 0, "parent user must be tombstoned"

print("Order o1 alive; user u1 tombstoned. Referential traceability preserved.")

e.close()
```

**What happens internally:** `storage.get_row()` returns `Some` for tombstoned rows, so FK validation at insert time passes. After sync, the parent is tombstoned but the child is alive. This is the most information-preserving choice — the application can query both `snapshot_state` and tombstones to surface the broken reference.

---

## Example 7: Three-Peer Convergence in Any Sync Order

Three peers converge to the same hash regardless of sync order.

```python
from adapter.adapter import Engine

e = Engine()
for peer in ["A", "B", "C"]:
    e.open_peer(peer)
    e.apply_schema(peer, ["CREATE TABLE items (id TEXT PRIMARY KEY, val TEXT)"])

# Each peer inserts a row offline
e.execute("A", "INSERT INTO items VALUES ('i1', 'alpha')")
e.execute("B", "INSERT INTO items VALUES ('i2', 'beta')")
e.execute("C", "INSERT INTO items VALUES ('i3', 'gamma')")

# Sync in one order
e.sync("A", "B")
e.sync("B", "C")
e.sync("A", "C")

hashes = {p: e.snapshot_hash(p) for p in ["A", "B", "C"]}
assert len(set(hashes.values())) == 1, "all three peers must converge"
print(f"All hashes equal: {hashes['A'][:16]}...")

e.close()
```

The same result holds for any permutation of sync order — `A↔C` first, then `B↔C`, then `A↔B` — because the merge is commutative and associative.

---

## Example 8: Running the Full Benchmark

```bash
# Quick self-check (all six axes, weighted score)
cd bench-harness/bench-p01-crdt
python3 self_check.py --adapter adapters.anvil:Engine --fk-policy tombstone

# Expected output:
#   AXIS                          PASS    WEIGHT
#   convergence                     PASS    0.30
#   uniqueness:users.email          PASS    0.20
#   fk                              PASS    0.15
#   cell-level:u1                   PASS    0.10
#   order-invariance                PASS    0.10
#   randomized                      PASS    0.15
#   WEIGHTED SCORE                1.00  / 1.00

# Stress test with custom seeds
python3 run.py --adapter adapters.anvil:Engine --fk-policy tombstone \
  --randomized-seeds 9999 31415 27182 16180 11235 \
  --rand-peers 5 --rand-ops 150 --out report.json

# Unit + integration tests
cargo test --lib --all
cargo test -p benchmark
```
