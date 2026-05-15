# Anvil Workflow Examples

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

# Initialize two independent replicas
e.open_peer("A")
e.open_peer("B")

schema = [
    "CREATE TABLE users (id TEXT PRIMARY KEY, email TEXT NOT NULL, name TEXT)",
]
e.apply_schema("A", schema)
e.apply_schema("B", schema)

# Each peer inserts a different row while offline from the other
e.execute("A", "INSERT INTO users (id, email, name) VALUES ('u1', 'alice@example.com', 'Alice')")
e.execute("B", "INSERT INTO users (id, email, name) VALUES ('u2', 'bob@example.com', 'Bob')")

# Bidirectional sync: A learns about u2, B learns about u1
e.sync("A", "B")

# Both peers now have the same two rows
hash_a = e.snapshot_hash("A")
hash_b = e.snapshot_hash("B")
assert hash_a == hash_b, "peers must converge"

state = e.snapshot_state("A")
print(state["users"])
# [{'id': 'u1', 'email': 'alice@example.com', 'name': 'Alice'},
#  {'id': 'u2', 'email': 'bob@example.com', 'name': 'Bob'}]

e.close()
```

---

## Example 2: Concurrent Updates and LWW Resolution

Two peers concurrently update the same cell. The update with the higher Lamport clock wins. After sync, both peers agree on the winning value.

```python
from adapter.adapter import Engine

e = Engine()
e.open_peer("A")
e.open_peer("B")

schema = ["CREATE TABLE users (id TEXT PRIMARY KEY, email TEXT NOT NULL, name TEXT)"]
e.apply_schema("A", schema)
e.apply_schema("B", schema)

# Seed both peers with the same row via sync
e.execute("A", "INSERT INTO users (id, email, name) VALUES ('u1', 'alice@example.com', 'Alice')")
e.sync("A", "B")

# Both peers update the same column concurrently (no sync between updates)
e.execute("A", "UPDATE users SET name = 'Alice A' WHERE id = 'u1'")
e.execute("B", "UPDATE users SET name = 'Alice B' WHERE id = 'u1'")

# Sync resolves the conflict: higher Lamport clock wins
# (B's clock was already advanced by the sync, so B's update has the higher counter)
e.sync("A", "B")

hash_a = e.snapshot_hash("A")
hash_b = e.snapshot_hash("B")
assert hash_a == hash_b, "peers must converge after conflict resolution"

state = e.snapshot_state("A")
winning_name = state["users"][0]["name"]
print(f"Winning name: {winning_name}")
# Both peers agree on the same name; the exact winner depends on clock values

e.close()
```

**Key point:** cell-level LWW means only the conflicting column is resolved. If A and B had concurrently updated different columns of the same row, both updates would survive independently.

---

## Example 3: Uniqueness Conflict — Two Peers Insert the Same Email

Two peers insert rows with the same UNIQUE email address while partitioned. After sync, one row becomes the canonical owner of that email. The other row is preserved as a loser in the uniqueness registry but filtered from normal query results.

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

# Both peers insert a different row with the same email while offline
e.execute("A", "INSERT INTO users (id, email, name) VALUES ('u1', 'shared@example.com', 'Alice')")
e.execute("B", "INSERT INTO users (id, email, name) VALUES ('u2', 'shared@example.com', 'Bob')")

# Sync: the uniqueness registry resolves the conflict
# Higher-versioned row wins ownership; the other becomes a loser
e.sync("A", "B")

hash_a = e.snapshot_hash("A")
hash_b = e.snapshot_hash("B")
assert hash_a == hash_b, "peers must converge"

state_a = e.snapshot_state("A")
# Exactly one visible row for 'shared@example.com' — the loser is hidden
visible = [r for r in state_a["users"] if r["email"] == "shared@example.com"]
assert len(visible) == 1, "exactly one owner visible"
print(f"Owner: {visible[0]['id']}")

e.close()
```

**Key point:** the losing row (`u1` or `u2`) is not deleted from internal storage; it is recorded in the `losers` list of the uniqueness claim. This allows conflict auditing and recovery if needed.

---

## Example 4: Delete and Sync (Tombstone Semantics)

One peer deletes a row while the other peer has a concurrent update to the same row. Delete-wins semantics ensure the deletion prevails after sync.

```python
from adapter.adapter import Engine

e = Engine()
e.open_peer("A")
e.open_peer("B")

schema = ["CREATE TABLE users (id TEXT PRIMARY KEY, email TEXT NOT NULL, name TEXT)"]
e.apply_schema("A", schema)
e.apply_schema("B", schema)

# Seed both peers
e.execute("A", "INSERT INTO users (id, email, name) VALUES ('u1', 'alice@example.com', 'Alice')")
e.sync("A", "B")

# Partition: A deletes the row, B updates it concurrently
e.execute("A", "DELETE FROM users WHERE id = 'u1'")
e.execute("B", "UPDATE users SET name = 'Alice Updated' WHERE id = 'u1'")

# Rejoin: delete-wins means the row is tombstoned on both peers
e.sync("A", "B")

hash_a = e.snapshot_hash("A")
hash_b = e.snapshot_hash("B")
assert hash_a == hash_b, "peers must converge"

state = e.snapshot_state("A")
visible_users = state.get("users", [])
assert len(visible_users) == 0, "deleted row must not appear in queries"
print("Row is tombstoned on both peers; snapshot hashes match")

e.close()
```

**Key point:** tombstoned rows are retained in internal storage (they are not physically removed until GC). The tombstone is included in the snapshot hash computation so that peers agree on which rows are deleted.

---

## Example 5: Running the Benchmark Self-Check

The benchmark validates multi-peer convergence, partition/re-merge scenarios, and BLAKE3 hash agreement. The self-check embedded in `adapter.py` covers the basic sync case. The full suite is in `crates/benchmark/`.

```bash
# Quick smoke test (built into adapter.py __main__)
python adapter/adapter.py
# Expected output:
# Hash A: <hex>
# Hash B: <hex>
# Hashes match: True
# Users on A: [{'id': 'u1', ...}, {'id': 'u2', ...}]
# Smoke test passed!

# Full benchmark validation suite
cargo test --lib

# Integration tests (randomized sync, partition simulation, snapshot validation)
cargo test -p benchmark
```

The benchmark test suite covers:

- **Randomized sync** (`randomized_sync.rs`) — multiple peers insert rows in random order, sync in random order, verify all hashes converge.
- **Partition simulation** (`partition_simulation.rs`) — peers operate independently for multiple rounds, then re-merge; verifies delete-wins and LWW hold across partition boundaries.
- **Snapshot validation** (`snapshot_validation.rs`) — verifies that BLAKE3 hashes are order-invariant: two peers that received the same operations in different orders produce identical hashes.
