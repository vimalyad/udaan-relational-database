use core::types::{Frontier, PeerId, Version};
use core::utils::{frontier_update, lamport_advance};

/// Per-peer Lamport clock manager.
pub struct LamportClock {
    pub peer_id: PeerId,
    pub counter: u64,
}

impl LamportClock {
    pub fn new(peer_id: impl Into<PeerId>) -> Self {
        Self {
            peer_id: peer_id.into(),
            counter: 0,
        }
    }

    /// Tick for a local write; returns new Version.
    pub fn tick(&mut self) -> Version {
        self.counter += 1;
        Version::new(self.counter, self.peer_id.clone())
    }

    /// Update clock on receiving a remote version.
    pub fn update(&mut self, remote: &Version) {
        self.counter = lamport_advance(self.counter, remote.counter);
    }

    /// Update clock from a remote frontier.
    pub fn update_from_frontier(&mut self, frontier: &Frontier) {
        if let Some(&remote_counter) = frontier.get(&self.peer_id) {
            self.counter = self.counter.max(remote_counter);
        }
        for (_, &cnt) in frontier.iter() {
            self.counter = self.counter.max(cnt);
        }
    }

    /// Export this peer's current counter into a frontier.
    pub fn to_frontier(&self) -> Frontier {
        let mut f = Frontier::new();
        frontier_update(&mut f, &self.peer_id, self.counter);
        f
    }
}
