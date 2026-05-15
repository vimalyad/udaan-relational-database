//! Replication state management: wraps storage + CRDT state for a single peer.

use core::types::{Frontier, PeerId};
use crdt::{LamportClock, TombstoneStore, UniquenessRegistry};
use storage::{SchemaStore, StorageEngine};

pub struct ReplicaState {
    pub peer_id: PeerId,
    pub clock: LamportClock,
    pub storage: StorageEngine,
    pub schemas: SchemaStore,
    pub tombstones: TombstoneStore,
    pub uniqueness: UniquenessRegistry,
    pub frontier: Frontier,
}

impl ReplicaState {
    pub fn new(peer_id: impl Into<PeerId>) -> Self {
        let peer_id = peer_id.into();
        Self {
            clock: LamportClock::new(peer_id.clone()),
            storage: StorageEngine::new(),
            schemas: SchemaStore::new(),
            tombstones: TombstoneStore::new(),
            uniqueness: UniquenessRegistry::new(),
            frontier: Frontier::new(),
            peer_id,
        }
    }

    pub fn current_version(&mut self) -> core::types::Version {
        self.clock.tick()
    }
}
