# Anvil Examples

Build first:

```bash
cargo build --release -p adapter
```

The Python adapter starts `target/release/anvil` and communicates with it over stdin/stdout. On Windows it uses `target\release\anvil.exe`.

---

## Basic Two-Peer Sync

```python
from adapter.adapter import Engine

e = Engine()
try:
    e.open_peer("A")
    e.open_peer("B")

    schema = ["CREATE TABLE users (id TEXT PRIMARY KEY, email TEXT NOT NULL UNIQUE, name TEXT)"]
    e.apply_schema("A", schema)
    e.apply_schema("B", schema)

    e.execute("A", "INSERT INTO users (id, email, name) VALUES ('u1', 'a@x.com', 'Alice')")
    e.execute("B", "INSERT INTO users (id, email, name) VALUES ('u2', 'b@x.com', 'Bob')")
    e.sync("A", "B")

    assert e.snapshot_hash("A") == e.snapshot_hash("B")
    print(e.snapshot_state("A"))
finally:
    e.close()
```

---

## Cell-Level Merge

Concurrent updates to different columns survive because each column is its own CRDT register.

```python
from adapter.adapter import Engine

e = Engine()
try:
    for p in ["A", "B"]:
        e.open_peer(p)
        e.apply_schema(p, ["CREATE TABLE users (id TEXT PRIMARY KEY, name TEXT, city TEXT)"])

    e.execute("A", "INSERT INTO users (id, name, city) VALUES ('u1', 'Alice', 'NYC')")
    e.sync("A", "B")

    e.execute("A", "UPDATE users SET name = 'Alice Smith' WHERE id = 'u1'")
    e.execute("B", "UPDATE users SET city = 'London' WHERE id = 'u1'")
    e.sync("A", "B")

    row = e.snapshot_state("A")["users"][0]
    assert row["name"] == "Alice Smith"
    assert row["city"] == "London"
finally:
    e.close()
```

---

## Unique Conflict

Two peers can locally accept the same unique value while partitioned. At merge, the uniqueness registry chooses one owner and preserves the losing accepted row by clearing the conflicting unique column.

```python
from adapter.adapter import Engine

e = Engine()
try:
    for p in ["A", "B"]:
        e.open_peer(p)
        e.apply_schema(p, ["CREATE TABLE users (id TEXT PRIMARY KEY, email TEXT UNIQUE, name TEXT)"])

    e.execute("A", "INSERT INTO users (id, email, name) VALUES ('u1', 'shared@x.com', 'Alice')")
    e.execute("B", "INSERT INTO users (id, email, name) VALUES ('u2', 'shared@x.com', 'Bob')")
    e.sync("A", "B")

    state = e.snapshot_state("A")
    assert len([r for r in state["users"] if r.get("email") == "shared@x.com"]) == 1
    assert len(state["users"]) == 2
finally:
    e.close()
```

---

## Tombstone FK Policy

FK checks are deferred until merge. Under tombstone policy, a child can remain visible even if its parent is tombstoned.

```python
from adapter.adapter import Engine

e = Engine()
try:
    for p in ["A", "B", "C"]:
        e.open_peer(p)
        e.apply_schema(p, [
            "CREATE TABLE users (id TEXT PRIMARY KEY, name TEXT)",
            "CREATE TABLE orders (id TEXT PRIMARY KEY, user_id TEXT REFERENCES users(id), item TEXT)",
        ])

    e.execute("A", "INSERT INTO users (id, name) VALUES ('u1', 'Alice')")
    e.sync("A", "B")
    e.sync("A", "C")

    e.execute("C", "DELETE FROM users WHERE id = 'u1'")
    e.execute("A", "INSERT INTO orders (id, user_id, item) VALUES ('o1', 'u1', 'Widget')")

    e.sync("A", "B")
    e.sync("B", "C")
    e.sync("A", "C")

    state = e.snapshot_state("A")
    assert any(o["id"] == "o1" for o in state.get("orders", []))
    assert not state.get("users", [])
finally:
    e.close()
```

---

## Full Benchmark

Linux/macOS:

```bash
cargo build --release -p adapter
cd bench-harness/bench-p01-crdt
python3 run.py --adapter adapters.anvil:Engine --fk-policy tombstone --out l3_report.json
```

Windows PowerShell:

```powershell
cargo build --release -p adapter
cd bench-harness\bench-p01-crdt
python run.py --adapter adapters.anvil:Engine --fk-policy tombstone --out l3_report.json
```

Verified result for this revision:

```text
core_score    1.00 / 1.00
stretch_score 0.75 / 1.00
final_score   0.90 / 1.00
```

---

## CI-Style Checks

Run from repository root:

```bash
cargo fmt --check
cargo test --workspace
cargo build --release -p adapter
python3 -m compileall -q bench-harness/bench-p01-crdt
```
