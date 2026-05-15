/// Benchmark metrics collected during a sync simulation run.
///
/// Tracks the cost of reaching quiescence across a set of peers:
/// how many sync rounds were required, how many rows were exchanged,
/// and the number of participating peers.
pub struct BenchMetrics {
    /// Number of sync rounds needed to reach quiescence.
    pub sync_rounds: usize,
    /// Total rows contained across all deltas exchanged.
    pub rows_exchanged: usize,
    /// Convergence time in microseconds (placeholder — always 0).
    pub convergence_time_us: u64,
    /// Number of peers participating in the simulation.
    pub peers: usize,
}

impl BenchMetrics {
    pub fn new(peers: usize, sync_rounds: usize, rows_exchanged: usize) -> Self {
        Self {
            peers,
            sync_rounds,
            rows_exchanged,
            convergence_time_us: 0,
        }
    }

    /// Returns a human-readable summary of the benchmark run.
    pub fn summary(&self) -> String {
        format!(
            "peers={} sync_rounds={} rows_exchanged={} convergence_time_us={}",
            self.peers, self.sync_rounds, self.rows_exchanged, self.convergence_time_us
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metrics_new_stores_fields() {
        let m = BenchMetrics::new(3, 5, 42);
        assert_eq!(m.peers, 3);
        assert_eq!(m.sync_rounds, 5);
        assert_eq!(m.rows_exchanged, 42);
        assert_eq!(m.convergence_time_us, 0);
    }

    #[test]
    fn summary_contains_all_fields() {
        let m = BenchMetrics::new(4, 2, 100);
        let s = m.summary();
        assert!(s.contains("peers=4"), "summary must include peer count");
        assert!(s.contains("sync_rounds=2"), "summary must include sync rounds");
        assert!(s.contains("rows_exchanged=100"), "summary must include rows exchanged");
        assert!(s.contains("convergence_time_us=0"), "summary must include convergence time");
    }
}
