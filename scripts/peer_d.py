"""
Anvil Demo — Peer D (Laptop 4)

Peer D concurrently updates the same column as Peer A on the same row,
demonstrating LWW cell-level conflict resolution. Also inserts new data.

Usage:
    cd udaan-relational-database
    python3 scripts/peer_d.py

Output: peer_d_state.json — D's local snapshot before sync
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
    e.open_peer("D")
    e.apply_schema("D", SCHEMA)

    print("[D] Setting up local state (offline)...")

    # D inserts its own users
    e.execute("D", "INSERT INTO users (id, email, name, city) VALUES ('u9',  'heidi@anvil.dev', 'Heidi', 'Nagpur')")
    e.execute("D", "INSERT INTO users (id, email, name, city) VALUES ('u10', 'ivan@anvil.dev',  'Ivan',  'Vadodara')")

    # D inserts orders
    e.execute("D", "INSERT INTO orders (id, user_id, item, qty) VALUES ('o6', 'u9',  'Router',  1)")
    e.execute("D", "INSERT INTO orders (id, user_id, item, qty) VALUES ('o7', 'u10', 'Speaker', 4)")

    # D updates carol's city — A and D both update u3 (different columns: D=city, C hasn't touched u3)
    e.execute("D", "UPDATE users SET city = 'Nashik' WHERE id = 'u3'")

    # D also does a bulk update across several orders
    e.execute("D", "UPDATE orders SET qty = 5 WHERE id = 'o6'")

    state = e.snapshot_state("D")
    h     = e.snapshot_hash("D")

    print(f"[D] Snapshot hash: {h[:16]}...")
    print(f"[D] Users:  {json.dumps(state.get('users',  []), indent=2)}")
    print(f"[D] Orders: {json.dumps(state.get('orders', []), indent=2)}")

    out = Path(__file__).parent.parent / "peer_d_state.json"
    out.write_text(json.dumps({"peer": "D", "hash": h, "state": state}, indent=2))
    print(f"\n[D] State written to {out}")

    e.close()

if __name__ == "__main__":
    run()
