use core::error::CrdtResult;
use core::types::SyncDelta;

/// Abstraction over sync transport.
/// In-process variant used for the benchmark; network variants for production.
pub trait Transport: Send + Sync {
    fn send(&self, peer_id: &str, delta: &SyncDelta) -> CrdtResult<()>;
    fn receive(&self, peer_id: &str) -> CrdtResult<Option<SyncDelta>>;
}

/// In-process transport: no-op (sync happens directly via function calls).
pub struct InProcessTransport;

impl Transport for InProcessTransport {
    fn send(&self, _peer_id: &str, _delta: &SyncDelta) -> CrdtResult<()> {
        Ok(())
    }

    fn receive(&self, _peer_id: &str) -> CrdtResult<Option<SyncDelta>> {
        Ok(None)
    }
}

/// JSON-over-TCP transport skeleton (Phase 10 future implementation).
pub struct TcpTransport {
    pub listen_addr: String,
}

impl TcpTransport {
    pub fn new(listen_addr: impl Into<String>) -> Self {
        Self { listen_addr: listen_addr.into() }
    }
}

impl Transport for TcpTransport {
    fn send(&self, _peer_id: &str, _delta: &SyncDelta) -> CrdtResult<()> {
        // Future: TCP send
        Ok(())
    }

    fn receive(&self, _peer_id: &str) -> CrdtResult<Option<SyncDelta>> {
        // Future: TCP receive
        Ok(None)
    }
}
