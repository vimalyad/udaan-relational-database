"""
Anvil Demo — Peer A (Laptop 1)

Peer A owns the users table seed data and some orders.
Run this BEFORE syncing with other peers.

Usage:
    cd udaan-relational-database
    python3 scripts/peer_a.py

Output: peer_a_state.json — A's local snapshot before sync
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
    e.open_peer("A")
    e.apply_schema("A", SCHEMA)

    print("[A] Inserting users and orders (offline)...")

    # Users
    e.execute("A", "INSERT INTO users (id, email, name, city) VALUES ('u1', 'alice@anvil.dev', 'Alice',   'Mumbai')")
    e.execute("A", "INSERT INTO users (id, email, name, city) VALUES ('u2', 'bob@anvil.dev',   'Bob',     'Delhi')")
    e.execute("A", "INSERT INTO users (id, email, name, city) VALUES ('u3', 'carol@anvil.dev', 'Carol',   'Pune')")

    # Orders
    e.execute("A", "INSERT INTO orders (id, user_id, item, qty) VALUES ('o1', 'u1', 'Laptop',  1)")
    e.execute("A", "INSERT INTO orders (id, user_id, item, qty) VALUES ('o2', 'u2', 'Monitor', 2)")

    # Update a row — A will have the latest version of u1.city
    e.execute("A", "UPDATE users SET city = 'Bangalore' WHERE id = 'u1'")

    state = e.snapshot_state("A")
    h     = e.snapshot_hash("A")

    print(f"[A] Snapshot hash: {h[:16]}...")
    print(f"[A] Users:  {json.dumps(state.get('users',  []), indent=2)}")
    print(f"[A] Orders: {json.dumps(state.get('orders', []), indent=2)}")

    out = Path(__file__).parent.parent / "peer_a_state.json"
    out.write_text(json.dumps({"peer": "A", "hash": h, "state": state}, indent=2))
    print(f"\n[A] State written to {out}")

    e.close()

if __name__ == "__main__":
    run()
