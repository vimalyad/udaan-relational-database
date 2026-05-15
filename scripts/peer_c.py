"""
Anvil Demo — Peer C (Laptop 3)

Peer C deletes a user (tombstone) while peer A has inserted an order
for that user — demonstrates FK tombstone policy.

Usage:
    cd udaan-relational-database
    python3 scripts/peer_c.py

Output: peer_c_state.json — C's local snapshot before sync
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
    e.open_peer("C")
    e.apply_schema("C", SCHEMA)

    print("[C] Setting up local state (offline)...")

    # C inserts its own users
    e.execute("C", "INSERT INTO users (id, email, name, city) VALUES ('u7', 'frank@anvil.dev', 'Frank', 'Ahmedabad')")
    e.execute("C", "INSERT INTO users (id, email, name, city) VALUES ('u8', 'grace@anvil.dev', 'Grace', 'Jaipur')")

    # C inserts a user that it will then delete
    # (to demonstrate tombstone propagation after sync)
    e.execute("C", "INSERT INTO users (id, email, name, city) VALUES ('u2', 'bob@anvil.dev', 'Bob', 'Delhi')")
    e.execute("C", "DELETE FROM users WHERE id = 'u2'")

    # C inserts orders for its own users
    e.execute("C", "INSERT INTO orders (id, user_id, item, qty) VALUES ('o4', 'u7', 'Tablet', 1)")
    e.execute("C", "INSERT INTO orders (id, user_id, item, qty) VALUES ('o5', 'u8', 'Phone',  2)")

    state = e.snapshot_state("C")
    h     = e.snapshot_hash("C")

    print(f"[C] Snapshot hash: {h[:16]}...")
    print(f"[C] Users:  {json.dumps(state.get('users',  []), indent=2)}")
    print(f"[C] Orders: {json.dumps(state.get('orders', []), indent=2)}")

    out = Path(__file__).parent.parent / "peer_c_state.json"
    out.write_text(json.dumps({"peer": "C", "hash": h, "state": state}, indent=2))
    print(f"\n[C] State written to {out}")

    e.close()

if __name__ == "__main__":
    run()
