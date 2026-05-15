use core::types::PeerId;

/// Peer session metadata.
#[derive(Debug, Clone)]
pub struct PeerSession {
    pub peer_id: PeerId,
    pub address: Option<String>,
    pub connected: bool,
}

impl PeerSession {
    pub fn new(peer_id: impl Into<PeerId>) -> Self {
        Self { peer_id: peer_id.into(), address: None, connected: false }
    }

    pub fn with_address(mut self, addr: impl Into<String>) -> Self {
        self.address = Some(addr.into());
        self
    }

    pub fn connect(&mut self) {
        self.connected = true;
    }

    pub fn disconnect(&mut self) {
        self.connected = false;
    }
}
