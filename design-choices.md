1. Database Model
 - Relational database with CRDT-based replication

 - Design Reasons:
   - preserves familiar SQL relational abstractions
   - supports tables, indexes, joins, and foreign keys
   - enables local-first/offline-first behavior
   - avoids centralized coordination during normal writes
   - eventual convergence across disconnected peers


2. Conflict Resolution Granularity
 - Cell-Level CRDTs

 - Reason:
   - preserves concurrent updates to different columns
   - benchmark explicitly discourages row-level LWW
   - avoids unnecessary overwrite of unrelated fields
   - enables deterministic fine-grained merges
   - minimizes logical write conflicts

 - Example:
   - peer A updates `name`
   - peer B updates `email`
   - merged row preserves both updates


3. Replication Model
 - rows/cells are mergeable states

 - Reason:
   - peers stay disconnected until sync
   - no causal broadcasts exist
   - no realtime op propagation
   - easier convergence reasoning
   - avoids dependency ordering issues of pure op-based CRDTs
   - anti-entropy synchronization maps naturally to state reconciliation
   - more robust under unreliable/delayed pairwise sync


4. Sync Architecture
 - Delta / Anti-entropy sync

 - Sync exchanges:
   - only changed rows/cells

 - Instead of:
   - entire database state

 - Model:
   - sync(a, b) pairwise reconciliation

 - Design Reasons:
   - minimizes bandwidth usage
   - scales better than full-state synchronization
   - supports offline peers reconnecting after long gaps
   - deterministic merge regardless of sync order
   - resilient to delayed or repeated synchronization
   - aligns with Dynamo/CouchDB-style anti-entropy replication


5. Peer Model
 - fully disconnected peers (before sync peers are completely independent)

 - Properties:
   - local-first writes
   - no central server
   - no shared ordering
   - no global coordinator
   - writes never blocked on network availability

 - Design Reasons:
   - enables offline-first operation
   - avoids availability bottlenecks
   - supports multi-region and peer-to-peer deployments
   - replicas may diverge temporarily but converge after synchronization
   - simplifies local write latency


6. DELETE vs UPDATE Conflict Semantics
 - Tombstone + Delete-Wins visibility semantics

 - DELETE operation:
   - does NOT physically remove the row
   - sets a tombstone / deleted flag on the row

 - Internal merge behavior:
   - concurrent cell updates are still merged normally
   - row data remains physically present for convergence and causal history

 - Visibility semantics:
   - deleted=true dominates query visibility
   - tombstoned rows are excluded from normal queries by default

 - Result:
   - concurrent updates are preserved internally
   - but the row remains logically deleted
   - prevents unintended row resurrection during sync

 - Design Reasons:
   - preserves causal history
   - simplifies FK conflict handling
   - ensures deterministic convergence
   - anti-entropy sync friendly
   - avoids zombie/resurrected rows
   - supports safe delayed synchronization
   - enables future tombstone garbage collection after convergence


7. Foreign Key Conflict Policy
 - Tombstone FK semantics

 - Behavior:
   - child rows survive even if parent is deleted
   - parent rows remain as tombstoned logical entities
   - FK references continue pointing to tombstoned parents
   - referential linkage remains causally preserved

 - Example:
   - peer A deletes user u1
   - peer B inserts order o1 referencing u1
   - after merge:
       - user u1 exists as tombstoned row
       - order o1 survives with reference intact

 - Query Semantics:
   - normal parent queries hide tombstoned rows
   - FK integrity still exists internally

 - Design Reasons:
   - preserves concurrent inserts under partition
   - avoids destructive cascade data loss
   - maintains referential causality
   - deterministic across all peers
   - naturally compatible with tombstone delete semantics
   - simplifies convergence proofs
   - avoids orphaned references
   - anti-entropy friendly


8. Uniqueness Constraint Semantics
 - Reservation / Claim-based uniqueness protocol

 - Goal:
   - enforce deterministic uniqueness across disconnected peers
   - without requiring a permanent central coordinator

 - Behavior:
   - unique values (e.g. email) are represented as distributed ownership claims
   - inserts first create a uniqueness reservation/claim
   - concurrent claims are merged deterministically
   - winner selected using deterministic ordering rules
     (Lamport clock + peer_id tie-break)

 - Conflict Resolution:
   - only one row becomes the canonical owner of the unique value
   - losing rows are NOT deleted silently
   - losing rows remain preserved internally as conflicted rows

 - Conflict Metadata:
   - conflicting rows are marked with uniqueness conflict metadata
   - conflicts remain recoverable and inspectable

 - Example:
   - peer A inserts:
       users(u1, email='alice@x.com')
   - peer B concurrently inserts:
       users(u2, email='alice@x.com')

   - after merge:
       - one row becomes canonical owner
       - losing row remains internally preserved with conflict metadata

 - Design Reasons:
   - pure CRDTs cannot guarantee uniqueness without coordination
   - deterministic winner selection guarantees convergence
   - preserves all causal history
   - satisfies benchmark requirement that loser remains recoverable
   - avoids silent data loss
   - compatible with anti-entropy synchronization
   - bounded metadata per unique value
   - no permanent centralized authority required


9. Versioning / Causality Tracking
 - Lamport Logical Clocks + deterministic peer_id tie-break

 - Version Structure:
   - every mutation is tagged as:
       (logical_counter, peer_id)

 - Clock Behavior:
   - each peer maintains a monotonically increasing logical counter
   - every local write increments the counter
   - sync operations update local counters using Lamport merge rules

 - Conflict Resolution:
   - larger Lamport counter wins
   - if counters are equal:
       deterministic peer_id tie-break resolves winner

 - Example:
   - (17, peerA)
   - (17, peerB)

   - if counters tie:
       deterministic peer_id ordering selects winner

 - Usage:
   - cell-level merge ordering
   - tombstone ordering
   - uniqueness conflict resolution
   - sync reconciliation ordering
   - deterministic state hashing

 - Design Reasons:
   - deterministic convergence across all replicas
   - avoids unsafe wall-clock timestamps
   - bounded metadata compared to vector clocks
   - simpler implementation and debugging
   - compatible with disconnected anti-entropy replication
   - avoids complexity of full causality graphs
   - sufficient for deterministic merge semantics required by benchmark
   - scalable under increasing peer count


10. Secondary Index Architecture
 - Deterministic derived secondary indexes

 - Index Model:
   - indexes are derived/materialized state
   - base table rows remain the authoritative source of truth

 - Structure:
   - Map<IndexKey, Set<RowID>>

 - Example:
   - ("u1", "pending") -> {o1, o7}

 - Behavior:
   - indexes are deterministically rebuilt or incrementally updated
     from merged canonical row state
   - tombstoned rows are excluded from normal index views
   - index state converges automatically because it is derived from
     converged table state

 - Query Semantics:
   - range queries operate on deterministic merged index state
   - all peers generate identical index ordering after convergence

 - Design Reasons:
   - avoids complex replicated index conflict resolution
   - prevents index drift/divergence between peers
   - deterministic index generation guarantees convergence
   - simplifies correctness reasoning
   - aligns naturally with state-based replication model
   - easier debugging and implementation
   - indexes become pure functions of merged replicated state
   - anti-entropy synchronization automatically repairs indexes


11. Tombstone Garbage Collection (GC)
 - Causal-stability-based tombstone garbage collection

 - Goal:
   - safely reclaim tombstone metadata
   - without risking deleted-row resurrection during delayed synchronization

 - Behavior:
   - tombstones are retained until all known peers have observed the delete
   - peers exchange synchronization frontier metadata during anti-entropy sync
   - tombstones are physically removed only after causal stabilization

 - GC Condition:
   - every known peer's observed Lamport frontier
     must be greater than or equal to the tombstone version

 - Example:
   - delete recorded at:
       (17, peerA)

   - tombstone can be garbage collected only if:
       peerB_seen >= (17, peerA)
       peerC_seen >= (17, peerA)

 - Metadata:
   - peers maintain synchronization frontier metadata
   - example:
       {
         peerA: 40,
         peerB: 28,
         peerC: 19
       }

 - Result:
   - deleted rows cannot accidentally resurrect
   - long-term metadata growth remains bounded
   - convergence guarantees remain intact

 - Design Reasons:
   - prevents premature tombstone deletion
   - preserves anti-entropy correctness
   - ensures deterministic convergence
   - compatible with disconnected peer model
   - avoids unbounded tombstone accumulation
   - aligns naturally with Lamport-based causality tracking
   - supports safe long-running replication
   - provides distributed causal stabilization before reclamation


12. Snapshot Hashing / Deterministic State Representation
 - Canonical deterministic serialization + cryptographic snapshot hashing

 - Goal:
   - guarantee bit-identical snapshot hashes across all converged peers
   - independent of sync order, merge history, or runtime implementation details

 - Canonicalization Rules:
   - tables sorted deterministically
   - rows sorted by primary key
   - columns sorted lexicographically
   - indexes sorted deterministically
   - tombstones and metadata serialized in stable order

 - Serialization:
   - canonical normalized serialization format
     (canonical JSON / CBOR / MessagePack)

 - Hashing:
   - snapshot hash generated using cryptographic hashing
     (SHA-256 or BLAKE3)

 - Included in Snapshot State:
   - merged row state
   - tombstones
   - uniqueness metadata
   - FK metadata
   - index state
   - causal/version metadata

 - Excluded from Snapshot State:
   - runtime memory layout
   - insertion order
   - local cache ordering
   - sync history artifacts

 - Result:
   - logically equivalent replicas always generate identical hashes
   - benchmark convergence validation becomes deterministic

 - Design Reasons:
   - guarantees deterministic convergence verification
   - prevents nondeterministic runtime serialization issues
   - independent of language/runtime implementation details
   - aligns naturally with deterministic merge semantics
   - simplifies debugging and reproducibility
   - enables reliable automated benchmark validation
   - snapshot hash becomes a cryptographic proof of convergence


13. Physical Storage Architecture
 - Materialized canonical row store

 - Storage Model:
   - merged canonical row state is persisted directly
   - storage layer acts as a local persistence engine
   - CRDT merge semantics remain implemented at the database layer

 - Physical Layout:
   - tables/
   - indexes/
   - tombstones/
   - uniqueness_claims/
   - peer_frontiers/
   - metadata/

 - Row Representation:
   - rows store:
       - cell values
       - Lamport versions
       - tombstone metadata
       - uniqueness conflict metadata
       - row-level metadata

 - Example:
   - users/u1:
       {
         cells: {
           email: {
             value: "alice@x.com",
             version: [17, "peerA"]
           }
         },
         deleted: false
       }

 - Sync Behavior:
   - synchronization exchanges only changed canonical rows/cells
   - sync operates using frontier/version comparisons

 - Index Behavior:
   - indexes are rebuilt/updated deterministically from canonical rows
   - indexes are not authoritative replicated state

 - Design Reasons:
   - simplifies deterministic snapshot generation
   - enables efficient reads and queries
   - avoids replay cost of event sourcing
   - easier debugging and implementation
   - naturally compatible with state-based CRDT replication
   - aligns with anti-entropy synchronization model
   - supports efficient canonical hashing
   - simplifies convergence reasoning

 - Technology Stack:
   - implementation language:
       Rust

   - execution/runtime target:
       WASM (WebAssembly)

 - Rust Design Reasons:
   - strong memory safety guarantees
   - deterministic low-level systems programming
   - excellent concurrency primitives
   - efficient serialization/deserialization ecosystem
   - suitable for distributed systems infrastructure
   - high-performance storage and synchronization engine implementation

 - WASM Design Reasons:
   - portable execution across browser/server/edge environments
   - deterministic execution characteristics
   - enables embedded local-first database deployment
   - allows peer replicas to run in heterogeneous environments
   - suitable for offline-first browser-based replication systems


14. Sync Protocol Architecture
 - Frontier-based deterministic anti-entropy synchronization

 - Sync Model:
   - peers exchange synchronization frontier/version summaries
   - only missing or outdated rows/cells are transferred
   - synchronization operates incrementally over changed state

 - Sync Complexity:
   - approximately O(changed_rows / delta_changes)
   - avoids O(total_database_size) full-state synchronization

 - Sync Flow:
   - Step 1:
       exchange peer_id + frontier metadata + optional snapshot hash

   - Step 2:
       determine missing/outdated versions using frontier comparison

   - Step 3:
       extract and transfer only changed rows/cells/tombstones/metadata

   - Step 4:
       perform deterministic CRDT merge resolution

   - Step 5:
       rebuild/update derived indexes and snapshot hash

   - Step 6:
       update synchronization frontier metadata

 - Frontier Metadata Example:
   - {
       peerA: 40,
       peerB: 18
     }

 - Merge Semantics:
   - merges use:
       - Lamport clock ordering
       - deterministic peer_id tie-breaks
       - tombstone semantics
       - uniqueness conflict resolution
       - FK conflict policies

 - Optimization:
   - optional snapshot hash equality shortcut
   - if hashes match:
       synchronization may be skipped entirely

 - Design Reasons:
   - bandwidth-efficient synchronization
   - deterministic convergence independent of sync order
   - scalable incremental replication
   - naturally compatible with disconnected peer model
   - aligns with Lamport frontier tracking
   - avoids full-state transfer overhead
   - supports efficient long-running replication
   - anti-entropy synchronization automatically repairs divergence


15. Transaction / Atomicity Model
 - Row-level atomicity + local replica transactions

 - Transaction Scope:
   - transactions are atomic within a single local replica
   - row-level updates are committed atomically
   - multi-row distributed serializability is intentionally not guaranteed

 - Guarantees:
   - single-row mutations are atomic
   - local transaction batches execute atomically on the local peer
   - deterministic eventual convergence after synchronization

 - Non-Guarantees:
   - no global distributed ACID transactions
   - no cross-peer serializable isolation
   - cross-row invariants may temporarily diverge under partitions

 - Merge Behavior:
   - concurrent distributed updates are resolved using CRDT merge semantics
   - conflicts reconcile deterministically after anti-entropy synchronization

 - Example:
   - peer A modifies account balance row
   - peer B concurrently updates related financial metadata
   - temporary divergence may exist until synchronization convergence

 - Design Reasons:
   - compatible with disconnected/offline-first peer model
   - avoids coordination-heavy distributed commit protocols
   - preserves availability during network partitions
   - aligns naturally with CRDT eventual consistency semantics
   - simplifies implementation and synchronization logic
   - avoids centralized transaction coordination
   - maintains deterministic convergence guarantees
   - provides practical local transactional semantics without sacrificing partition tolerance

 - CAP Theorem Orientation:
   - AP-oriented architecture
   - prioritizes availability and partition tolerance
   - relaxes strong global consistency in favor of deterministic eventual convergence