# CHANGES.md

## [Unreleased]

---

## Phase 1 — Repository Bootstrap

**Branch:** `feature/repository-bootstrap`

### What Changed

- Initialized Cargo workspace with 14 crates
- Implemented core shared types: `Version`, `Cell`, `Row`, `Tombstone`, `Frontier`, `UniquenessClaim`, `SyncDelta`, `TableSchema`
- Implemented Lamport clock engine (`crdt::clock`)
- Implemented deterministic cell/row/table merge semantics (`crdt::merge`)
- Implemented tombstone store with causal-stability GC (`crdt::tombstone`)
- Implemented uniqueness reservation/claim protocol (`crdt::uniqueness`)
- Implemented in-memory storage engine (`storage::engine`)
- Implemented schema store (`storage::schema_store`)
- Implemented canonical CBOR serialization helpers (`storage::serialization`)
- Implemented anti-entropy sync skeleton (`sync`)
- Implemented BLAKE3 snapshot hasher (`hashing`)
- Implemented deterministic secondary index structures (`index`)
- Implemented local transaction buffer (`transaction`)
- Implemented tombstone GC logic (`gc`)
- Implemented peer metadata registry (`metadata`)
- Implemented subprocess adapter binary skeleton (`adapter`)
- Configured rustfmt

### Modules Affected

All crates (initial implementation)

### Known Limitations

- SQL parser integrated (sqlparser-rs) but executor not yet wired to storage
- WASM bindings are placeholders
- Networking is a stub (in-process transport only)
- Persistence is in-memory only (no disk storage)
- Python adapter binary responds to commands but SQL execution returns null

### Tests

- 11 unit tests passing: cell merge invariants (associativity, commutativity, idempotency), uniqueness claim protocol

### Future TODOs

- Phase 2: Wire SQL executor to storage engine
- Phase 3: Add disk persistence
- Phase 4: Full sync engine integration tests
- Phase 6: Complete SQL execution (INSERT/UPDATE/DELETE/SELECT)
- Phase 11: Python adapter integration with benchmark harness
