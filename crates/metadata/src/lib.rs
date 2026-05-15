//! Peer metadata management.

use core::types::{Frontier, PeerId};
use std::collections::BTreeMap;

/// Tracks known peers and their frontiers.
#[derive(Debug, Clone, Default)]
pub struct PeerRegistry {
    /// peer_id -> frontier
    peers: BTreeMap<PeerId, Frontier>,
}

impl PeerRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn update_peer_frontier(&mut self, peer_id: &str, frontier: Frontier) {
        let entry = self.peers.entry(peer_id.to_string()).or_default();
        for (p, &cnt) in &frontier {
            let e = entry.entry(p.clone()).or_insert(0);
            if cnt > *e {
                *e = cnt;
            }
        }
    }

    pub fn get_frontier(&self, peer_id: &str) -> Option<&Frontier> {
        self.peers.get(peer_id)
    }

    pub fn all_peer_ids(&self) -> Vec<&PeerId> {
        self.peers.keys().collect()
    }

    /// Global frontier: min across all known peers (for GC purposes).
    pub fn global_min_frontier(&self) -> Frontier {
        if self.peers.is_empty() {
            return Frontier::new();
        }
        let mut result: Frontier = Frontier::new();
        let mut first = true;
        for frontier in self.peers.values() {
            if first {
                result = frontier.clone();
                first = false;
            } else {
                result.retain(|peer, cnt| {
                    if let Some(&other) = frontier.get(peer) {
                        *cnt = (*cnt).min(other);
                        true
                    } else {
                        false
                    }
                });
            }
        }
        result
    }
}
