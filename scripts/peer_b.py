"""
Anvil Demo — Peer B (Laptop 2)

Peer B inserts new users and updates a city that A also updated —
demonstrating LWW conflict resolution after sync.

Usage:
    cd udaan-relational-database
    python3 scripts/peer_b.py

Output: peer_b_state.json — B's local snapshot before sync
"""

import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))
from adapter.adapter import Engine

SCHEMA = [
    "CREATE TABLE users  (id TEXT PRIMARY KEY, email TEXT NOT NULL UNIQUE, name TEXT, city TEXT)",
    "CREATE TABLE orders (id TEXT PRIMARY KEY, user_id TEXT REFERENCES users(id), item TEXT, qty INTEGER)",
    "CREATE INDEX idx_orders_user ON orders (user_id)",
]

def run():
    e = Engine()
    e.open_peer("B")
    e.apply_schema("B", SCHEMA)

    print("[B] Inserting users and orders (offline)...")

    # B inserts new users
    e.execute("B", "INSERT INTO users (id, email, name, city) VALUES ('u4', 'dave@anvil.dev',  'Dave',  'Chennai')")
    e.execute("B", "INSERT INTO users (id, email, name, city) VALUES ('u5', 'eva@anvil.dev',   'Eva',   'Hyderabad')")

    # B also inserts an order for u1 (inserted by A) — FK works across partition
    e.execute("B", "INSERT INTO orders (id, user_id, item, qty) VALUES ('o3', 'u1', 'Keyboard', 3)")

    # B updates u1's city — this will conflict with A's update (LWW resolves it)
    e.execute("B", "UPDATE users SET city = 'Kolkata' WHERE id = 'u1'")

    # B also tries to claim the same email as A's u1 — demonstrates uniqueness conflict
    # (This row will lose to u1 after sync because u1 was inserted first by A)
    e.execute("B", "INSERT INTO users (id, email, name, city) VALUES ('u6', 'alice@anvil.dev', 'Alice2', 'Surat')")

    state = e.snapshot_state("B")
    h     = e.snapshot_hash("B")

    print(f"[B] Snapshot hash: {h[:16]}...")
    print(f"[B] Users:  {json.dumps(state.get('users',  []), indent=2)}")
    print(f"[B] Orders: {json.dumps(state.get('orders', []), indent=2)}")

    out = Path(__file__).parent.parent / "peer_b_state.json"
    out.write_text(json.dumps({"peer": "B", "hash": h, "state": state}, indent=2))
    print(f"\n[B] State written to {out}")

    e.close()

if __name__ == "__main__":
    run()
