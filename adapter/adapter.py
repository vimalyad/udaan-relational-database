"""
Anvil CRDT Engine — Python Benchmark Adapter

Bridges the benchmark harness (adapter.py interface) to the Rust engine
via subprocess JSON-RPC protocol.

Usage:
    python self_check.py --adapter adapter.adapter:Engine --fk-policy tombstone
"""

import json
import os
import subprocess
import sys
from pathlib import Path


def _find_binary():
    """Find the anvil binary in release or debug build."""
    root = Path(__file__).parent.parent
    exe = ".exe" if sys.platform == "win32" else ""
    candidates = [
        root / "target" / "release" / f"anvil{exe}",
        root / "target" / "debug" / f"anvil{exe}",
    ]
    for p in candidates:
        if p.exists():
            return str(p)
    raise FileNotFoundError(
        f"anvil binary not found. Run: cargo build --release\n"
        f"Searched: {[str(p) for p in candidates]}"
    )


class _AnvilProcess:
    """Manages a single anvil subprocess instance."""

    def __init__(self, binary_path: str):
        self._proc = subprocess.Popen(
            [binary_path],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            bufsize=1,  # line-buffered
        )

    def _send(self, cmd: dict) -> dict:
        line = json.dumps(cmd) + "\n"
        self._proc.stdin.write(line.encode())
        self._proc.stdin.flush()
        response_line = self._proc.stdout.readline()
        if not response_line:
            stderr = self._proc.stderr.read()
            raise RuntimeError(f"anvil process died: {stderr.decode()}")
        resp = json.loads(response_line.decode().strip())
        if resp.get("status") == "error":
            raise RuntimeError(f"anvil error: {resp.get('message')}")
        return resp.get("result")

    def open_peer(self, peer_id: str):
        self._send({"cmd": "OpenPeer", "args": {"peer_id": peer_id}})

    def apply_schema(self, peer_id: str, stmts: list[str]):
        self._send({"cmd": "ApplySchema", "args": {"peer_id": peer_id, "stmts": stmts}})

    def execute(self, peer_id: str, sql: str, params: list = None):
        return self._send({
            "cmd": "Execute",
            "args": {"peer_id": peer_id, "sql": sql, "params": params or []}
        })

    def sync(self, peer_a: str, peer_b: str):
        self._send({"cmd": "Sync", "args": {"peer_a": peer_a, "peer_b": peer_b}})

    def snapshot_hash(self, peer_id: str) -> str:
        return self._send({"cmd": "SnapshotHash", "args": {"peer_id": peer_id}})

    def snapshot_state(self, peer_id: str) -> dict:
        return self._send({"cmd": "SnapshotState", "args": {"peer_id": peer_id}})

    def close(self):
        try:
            self._send({"cmd": "Close", "args": {}})
        except Exception:
            pass
        try:
            self._proc.stdin.close()
            self._proc.wait(timeout=5)
        except Exception:
            self._proc.kill()


class Engine:
    """
    Benchmark adapter implementation.
    Implements the interface defined in bench-harness/bench-p01-crdt/adapter.py
    """

    def __init__(self):
        binary = _find_binary()
        self._proc = _AnvilProcess(binary)

    def open_peer(self, peer_id: str):
        """Initialize a new peer replica."""
        self._proc.open_peer(peer_id)

    def apply_schema(self, peer_id: str, stmts: list[str]):
        """Apply DDL statements (CREATE TABLE, CREATE INDEX) to a peer."""
        self._proc.apply_schema(peer_id, stmts)

    def execute(self, peer_id: str, sql: str, params: tuple = ()):
        """Execute a SQL statement (INSERT/UPDATE/DELETE/SELECT) on a peer."""
        return self._proc.execute(peer_id, sql, list(params))

    def sync(self, peer_a: str, peer_b: str):
        """Pairwise bidirectional sync between two peers."""
        self._proc.sync(peer_a, peer_b)

    def snapshot_hash(self, peer_id: str) -> str:
        """Return BLAKE3 hex hash of deterministic peer state."""
        return self._proc.snapshot_hash(peer_id)

    def snapshot_state(self, peer_id: str) -> dict:
        """Return tables->rows mapping for the peer (visible rows only)."""
        return self._proc.snapshot_state(peer_id)

    def close(self):
        """Shutdown all peers and the engine process."""
        self._proc.close()


if __name__ == "__main__":
    # Quick smoke test
    e = Engine()
    e.open_peer("A")
    e.open_peer("B")

    schema = [
        "CREATE TABLE users (id TEXT PRIMARY KEY, email TEXT NOT NULL, name TEXT)",
        "CREATE TABLE orders (id TEXT PRIMARY KEY, user_id TEXT NOT NULL, status TEXT NOT NULL, total_cents INTEGER NOT NULL)",
    ]
    e.apply_schema("A", schema)
    e.apply_schema("B", schema)

    e.execute("A", "INSERT INTO users (id, email, name) VALUES ('u1', 'alice@x.com', 'Alice')")
    e.execute("B", "INSERT INTO users (id, email, name) VALUES ('u2', 'bob@x.com', 'Bob')")

    e.sync("A", "B")

    hash_a = e.snapshot_hash("A")
    hash_b = e.snapshot_hash("B")
    print(f"Hash A: {hash_a}")
    print(f"Hash B: {hash_b}")
    print(f"Hashes match: {hash_a == hash_b}")

    state = e.snapshot_state("A")
    print(f"Users on A: {state.get('users', [])}")

    e.close()
    print("Smoke test passed!")
