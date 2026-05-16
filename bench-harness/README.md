# Benchmark Harness

The active benchmark is:

```text
bench-harness/bench-p01-crdt
```

Build from repository root:

```bash
cargo build --release -p adapter
```

Run the full L3 benchmark:

```bash
cd bench-harness/bench-p01-crdt
python3 run.py --adapter adapters.anvil:Engine --fk-policy tombstone --out l3_report.json
```

Fast smoke run:

```bash
cd bench-harness/bench-p01-crdt
python3 run.py --adapter adapters.anvil:Engine --fk-policy tombstone --long-run-ops 50 --out smoke_report.json
```

Verified score for the current engine revision:

```text
core_score     1.00 / 1.00
stretch_score  0.75 / 1.00
final_score    0.90 / 1.00
```
