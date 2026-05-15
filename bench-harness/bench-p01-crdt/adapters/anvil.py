"""
Anvil CRDT Engine — Benchmark Adapter

Bridges the Anvil Rust engine to the P-01 benchmark harness via subprocess.
"""
from __future__ import annotations

import json
import os
import subprocess
import sys
from pathlib import Path
from typing import Any

# Import the base Adapter class
sys.path.insert(0, str(Path(__file__).parent.parent))
from adapter import Adapter


def _find_binary():
    """Find the anvil binary relative to the repo root."""
    # bench-harness/bench-p01-crdt/adapters/ -> ../../.. -> repo root
    repo_root = Path(__file__).parent.parent.parent.parent
    candidates = [
        repo_root / "target" / "release" / "anvil",
        repo_root / "target" / "debug" / "anvil",
    ]
    for p in candidates:
        if p.exists():
            return str(p)
    raise FileNotFoundError(
        f"anvil binary not found. Run:\n  cargo build --release -p adapter\n"
        f"Searched: {[str(p) for p in candidates]}"
    )


class _AnvilProcess:
    """Manages the anvil subprocess."""

    def __init__(self):
        binary = _find_binary()
        self._proc = subprocess.Popen(
            [binary],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )

    def _send(self, cmd: dict) -> Any:
        line = json.dumps(cmd) + "\n"
        self._proc.stdin.write(line.encode("utf-8"))
        self._proc.stdin.flush()
        response_line = self._proc.stdout.readline()
        if not response_line:
            stderr = self._proc.stderr.read()
            raise RuntimeError(f"anvil process died. stderr: {stderr.decode()}")
        resp = json.loads(response_line.decode("utf-8").strip())
        if resp.get("status") == "error":
            raise RuntimeError(f"anvil error: {resp.get('message')}")
        return resp.get("result")

    def open_peer(self, peer_id: str):
        self._send({"cmd": "OpenPeer", "args": {"peer_id": peer_id}})

    def apply_schema(self, peer_id: str, stmts: list[str]):
        self._send({"cmd": "ApplySchema", "args": {"peer_id": peer_id, "stmts": stmts}})

    def execute(self, peer_id: str, sql: str, params: list):
        return self._send({"cmd": "Execute", "args": {"peer_id": peer_id, "sql": sql, "params": params}})

    def sync(self, peer_a: str, peer_b: str):
        self._send({"cmd": "Sync", "args": {"peer_a": peer_a, "peer_b": peer_b}})

    def snapshot_hash(self, peer_id: str) -> str:
        result = self._send({"cmd": "SnapshotHash", "args": {"peer_id": peer_id}})
        return result

    def snapshot_state(self, peer_id: str) -> dict:
        result = self._send({"cmd": "SnapshotState", "args": {"peer_id": peer_id}})
        return result or {}

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


def _substitute_params(sql: str, params: tuple) -> str:
    """Replace ? placeholders with properly quoted SQL literals."""
    if not params:
        return sql
    result = []
    param_iter = iter(params)
    i = 0
    while i < len(sql):
        if sql[i] == '?' and (i == 0 or sql[i-1] != "'"):
            try:
                val = next(param_iter)
            except StopIteration:
                result.append('?')
                i += 1
                continue
            if val is None:
                result.append('NULL')
            elif isinstance(val, bool):
                result.append('1' if val else '0')
            elif isinstance(val, int):
                result.append(str(val))
            elif isinstance(val, float):
                result.append(str(int(val)))
            elif isinstance(val, str):
                escaped = val.replace("'", "''")
                result.append(f"'{escaped}'")
            elif isinstance(val, bytes):
                result.append(f"X'{val.hex()}'")
            else:
                result.append(f"'{val}'")
        else:
            result.append(sql[i])
        i += 1
    return ''.join(result)


class Engine(Adapter):
    """
    Anvil CRDT engine adapter for the P-01 benchmark.

    FK policy: tombstone
    - Parent deletion does not cascade to child rows.
    - Parent is preserved as tombstone; child FK reference remains valid.
    """

    def __init__(self):
        self._proc = _AnvilProcess()

    def open_peer(self, peer_id: str) -> None:
        self._proc.open_peer(peer_id)

    def apply_schema(self, peer_id: str, stmts: list[str]) -> None:
        self._proc.apply_schema(peer_id, stmts)

    def execute(self, peer_id: str, sql: str, params: tuple[Any, ...] = ()) -> None:
        # Substitute ? placeholders with actual values (Rust engine doesn't support ? params)
        sql_with_values = _substitute_params(sql, params)
        self._proc.execute(peer_id, sql_with_values, [])

    def sync(self, peer_a: str, peer_b: str) -> None:
        self._proc.sync(peer_a, peer_b)

    def snapshot_hash(self, peer_id: str) -> str:
        return self._proc.snapshot_hash(peer_id)

    def snapshot_state(self, peer_id: str) -> dict[str, list[dict[str, Any]]]:
        return self._proc.snapshot_state(peer_id)

    def close(self) -> None:
        self._proc.close()
