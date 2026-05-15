/// Benchmark metrics collected during a sync simulation run.
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
