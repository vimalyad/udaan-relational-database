use crate::delta::{apply_delta, extract_delta};
use core::error::CrdtResult;
use replication::ReplicaState;

/// Full pairwise bidirectional sync.
/// Extracts deltas for both sides, then applies both.
/// Run repeatedly until quiescent (no new rows in extracted delta).
pub fn sync_peers(a: &mut ReplicaState, b: &mut ReplicaState) -> CrdtResult<()> {
    let delta_for_b = extract_delta(a, &b.frontier);
    let delta_for_a = extract_delta(b, &a.frontier);

    apply_delta(b, &delta_for_b)?;
    apply_delta(a, &delta_for_a)?;

    Ok(())
}

/// Sync to quiescence: run sync_peers until no new state is exchanged.
/// For the reference scenario this typically takes 1-2 rounds.
pub fn sync_to_quiescence(a: &mut ReplicaState, b: &mut ReplicaState) -> CrdtResult<usize> {
    let mut rounds = 0;
    loop {
        let delta_for_b = extract_delta(a, &b.frontier);
        let delta_for_a = extract_delta(b, &a.frontier);
        let any_new = !delta_for_b.rows.is_empty()
            || !delta_for_b.tombstones.is_empty()
            || !delta_for_a.rows.is_empty()
            || !delta_for_a.tombstones.is_empty();

        apply_delta(b, &delta_for_b)?;
        apply_delta(a, &delta_for_a)?;
        rounds += 1;

        if !any_new || rounds > 100 {
            break;
        }
    }
    Ok(rounds)
}
