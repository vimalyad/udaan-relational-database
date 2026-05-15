use crate::types::{Frontier, PeerId, Version};
use std::collections::BTreeMap;

/// Advance a Lamport clock: max(local, received) + 1 for local events,
/// or just update-to-max on receive.
pub fn lamport_advance(local: u64, received: u64) -> u64 {
    local.max(received) + 1
}

/// Tick the local counter for a local write.
pub fn lamport_tick(counter: &mut u64) -> u64 {
    *counter += 1;
    *counter
}

/// Merge two frontiers: take the max counter for each peer.
pub fn merge_frontiers(a: &Frontier, b: &Frontier) -> Frontier {
    let mut result: Frontier = BTreeMap::new();
    for (peer, &cnt) in a.iter().chain(b.iter()) {
        let entry = result.entry(peer.clone()).or_insert(0);
        if cnt > *entry {
            *entry = cnt;
        }
    }
    result
}

/// Update a peer's entry in a frontier.
pub fn frontier_update(frontier: &mut Frontier, peer: &PeerId, counter: u64) {
    let entry = frontier.entry(peer.clone()).or_insert(0);
    if counter > *entry {
        *entry = counter;
    }
}

/// Check if frontier `a` has seen everything in frontier `b`.
/// Returns true if for every peer in b, a[peer] >= b[peer].
pub fn frontier_dominates(a: &Frontier, b: &Frontier) -> bool {
    b.iter().all(|(peer, &cnt)| a.get(peer).copied().unwrap_or(0) >= cnt)
}

/// Select the winning Version using Lamport ordering (higher wins).
/// Tie-break: higher peer_id (lexicographic) wins.
pub fn version_wins(challenger: &Version, incumbent: &Version) -> bool {
    challenger > incumbent
}

/// Canonical serialization of a value to bytes for hashing.
/// Uses a deterministic encoding that is stable across machines.
pub fn canonical_bytes<T: serde::Serialize>(value: &T) -> Result<Vec<u8>, String> {
    let mut buf = Vec::new();
    ciborium::into_writer(value, &mut buf).map_err(|e| e.to_string())?;
    Ok(buf)
}

/// BLAKE3 hash of canonical bytes.
pub fn blake3_hash(data: &[u8]) -> [u8; 32] {
    *blake3::hash(data).as_bytes()
}

/// Hash of a serializable value.
pub fn hash_value<T: serde::Serialize>(value: &T) -> Result<[u8; 32], String> {
    let bytes = canonical_bytes(value)?;
    Ok(blake3_hash(&bytes))
}

/// Hex-encode a 32-byte hash.
pub fn hash_to_hex(hash: &[u8; 32]) -> String {
    hex::encode(hash)
}
