# CRDT-Native Relational Database

**Team:** Udaan

Anvil is a multi-writer embedded relational database prototype. Each peer writes locally, syncs by exchanging CRDT deltas, and converges without a coordinator. The SQL surface is intentionally SQLite-like, while the storage layer keeps CRDT metadata for cell-level merge, tombstones, uniqueness claims, and FK policy enforcement.

**Verified benchmark result:** `0.9000 / 1.0000` on the full `anvil-2026-p01-L3-final` runner.

Current breakdown:

| Area | Result |
|---|---|
| Core benchmark axes | `1.00 / 1.00` |
| L3 stretch axes | `0.75 / 1.00` |
| Final weighted score | `0.90 / 1.00` |

Known remaining limitation: one multi-level FK cascade edge case remains in the stretch suite. The current implementation favors a causal/generalized FK interpretation over benchmark-specific hardcoding.

---

## Quick Start

Prerequisites:

- Rust toolchain from `rust-toolchain.toml`
- Python 3.10+

Linux/macOS:

```bash
git clone https://github.com/vimalyad/udaan-relational-database
cd udaan-relational-database

cargo build --release -p adapter
cd bench-harness/bench-p01-crdt
python3 run.py --adapter adapters.anvil:Engine --fk-policy tombstone --out l3_report.json
```

Windows PowerShell:

```powershell
git clone https://github.com/vimalyad/udaan-relational-database
cd udaan-relational-database

cargo build --release -p adapter
cd bench-harness\bench-p01-crdt
python run.py --adapter adapters.anvil:Engine --fk-policy tombstone --out l3_report.json
```

The adapter automatically looks for `target/release/anvil` on Linux/macOS and `target\release\anvil.exe` on Windows.

---

## Run Checks

From the repository root:

```bash
cargo fmt --check
cargo test --workspace
cargo build --release -p adapter
```

Full L3 benchmark:

```bash
cd bench-harness/bench-p01-crdt
python3 run.py --adapter adapters.anvil:Engine --fk-policy tombstone --out l3_report.json
```

Fast smoke benchmark:

```bash
cd bench-harness/bench-p01-crdt
python3 run.py --adapter adapters.anvil:Engine --fk-policy tombstone --long-run-ops 50 --out smoke_report.json
```

Docker:

```bash
docker build -t anvil .
docker run --rm anvil
```

---

## Run Your Own Testcases

Use the Python adapter from a script:

```python
from adapter.adapter import Engine

e = Engine()
try:
    for peer in ["A", "B"]:
        e.open_peer(peer)
        e.apply_schema(peer, [
            "CREATE TABLE users (id TEXT PRIMARY KEY, email TEXT NOT NULL UNIQUE, name TEXT)",
        ])

    e.execute("A", "INSERT INTO users (id, email, name) VALUES ('u1', 'a@x.com', 'Alice')")
    e.execute("B", "INSERT INTO users (id, email, name) VALUES ('u2', 'b@x.com', 'Bob')")
    e.sync("A", "B")

    assert e.snapshot_hash("A") == e.snapshot_hash("B")
    print(e.snapshot_state("A"))
finally:
    e.close()
```

Run it after building:

```bash
cargo build --release -p adapter
python3 your_test.py
```

See [EXAMPLES.md](EXAMPLES.md) for more scenarios.

---

## What It Supports

| Capability | Implementation |
|---|---|
| DDL | `CREATE TABLE`, `CREATE INDEX`, primary keys, `UNIQUE`, `NOT NULL`, defaults, FK declarations |
| DML | `INSERT`, `UPDATE`, `DELETE`, `SELECT ... WHERE ... ORDER BY ... LIMIT` |
| Merge unit | Per-cell LWW registers with Lamport versions |
| Deletes | Delete-wins rows with tombstone metadata |
| Sync | Frontier-based delta extraction and idempotent apply |
| Uniqueness | Schema-driven claim registry for single-column and composite unique constraints |
| FK policy | Deferred merge-time handling for tombstone, cascade, and orphan semantics |
| Hashing | BLAKE3 over semantic state for deterministic convergence checks |
| Adapter | JSON-lines subprocess bridge used by the Python benchmark harness |

---

## Architecture Summary

```
Python benchmark / custom tests
        |
        | newline-delimited JSON
        v
target/release/anvil
        |
        v
EngineHost
  - peer id -> ReplicaState
  - SQL executor
  - sync/apply_delta
  - post-merge integrity enforcement
        |
        v
ReplicaState
  - StorageEngine
  - SchemaStore
  - Lamport clock + frontier
  - TombstoneStore
  - UniquenessRegistry
```

More detail is in [ARCHITECTURE.md](ARCHITECTURE.md).

---

## Crate Map

| Crate | Responsibility |
|---|---|
| `core` | Shared types: rows, cells, versions, tombstones, schema, sync deltas |
| `crdt` | Lamport clock, merge rules, tombstones, uniqueness registry |
| `storage` | Deterministic in-memory row and schema storage |
| `replication` | Per-peer replica state |
| `sync` | Delta extraction and apply |
| `sql` | SQL parsing/execution over CRDT state |
| `index` | Deterministic secondary indexes |
| `hashing` | BLAKE3 snapshot hashing |
| `adapter` | `anvil` JSON-RPC subprocess binary |
| `benchmark` | Rust-side validation tests |

---

## License

MIT - see [LICENSE](LICENSE).
