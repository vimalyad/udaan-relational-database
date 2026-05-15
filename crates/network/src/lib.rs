//! Transport abstraction for sync. Phase 10 placeholder.
//! The sync engine works in-process (no network needed for the benchmark).
//! This module provides the interface for future transport implementations.

use core::types::SyncDelta;
use core::error::CrdtResult;

pub trait Transport: Send + Sync {
    fn send(&self, peer_id: &str, delta: &SyncDelta) -> CrdtResult<()>;
    fn receive(&self, peer_id: &str) -> CrdtResult<Option<SyncDelta>>;
}

/// In-process transport for testing — deltas exchanged via direct memory.
pub struct InProcessTransport;

impl Transport for InProcessTransport {
    fn send(&self, _peer_id: &str, _delta: &SyncDelta) -> CrdtResult<()> {
        Ok(())
    }

    fn receive(&self, _peer_id: &str) -> CrdtResult<Option<SyncDelta>> {
        Ok(None)
    }
}
