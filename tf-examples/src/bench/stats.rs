//! Latency recording (HdrHistogram) and the machine-readable result line.

use hdrhistogram::Histogram;

/// Per-task latency recorder. Tracks 1us..60s with 3 significant figures.
pub struct LatencyRecorder {
    pub hist: Histogram<u64>,
    pub count: u64,
    pub errors: u64,
}

impl LatencyRecorder {
    pub fn new() -> Self {
        Self {
            hist: Histogram::new_with_bounds(1, 60_000_000, 3).expect("histogram bounds"),
            count: 0,
            errors: 0,
        }
    }

    /// Record one successful round-trip latency in microseconds.
    pub fn record_us(&mut self, us: u64) {
        let v = us.clamp(1, 60_000_000);
        let _ = self.hist.record(v);
        self.count += 1;
    }

    pub fn error(&mut self) {
        self.errors += 1;
    }

    pub fn merge(&mut self, other: &LatencyRecorder) {
        let _ = self.hist.add(&other.hist);
        self.count += other.count;
        self.errors += other.errors;
    }
}

impl Default for LatencyRecorder {
    fn default() -> Self {
        Self::new()
    }
}

/// Final result of one (target, connections, payload) run.
pub struct BenchResult {
    pub target: String,
    pub conns: usize,
    pub payload: usize,
    pub duration_s: f64,
    pub ok_conns: usize,
    pub rps: f64,
    pub p50_us: u64,
    pub p99_us: u64,
    pub p999_us: u64,
    pub max_us: u64,
    pub count: u64,
    pub errors: u64,
    pub cpu_pct: f64,
    pub peak_rss_mb: f64,
}

impl BenchResult {
    /// One line that the orchestrator script greps and turns into a table.
    pub fn result_line(&self) -> String {
        format!(
            "RESULT target={} conns={} payload={} dur={:.2} ok_conns={} rps={:.0} \
p50_us={} p99_us={} p999_us={} max_us={} count={} errors={} cpu_pct={:.1} peak_rss_mb={:.1}",
            self.target,
            self.conns,
            self.payload,
            self.duration_s,
            self.ok_conns,
            self.rps,
            self.p50_us,
            self.p99_us,
            self.p999_us,
            self.max_us,
            self.count,
            self.errors,
            self.cpu_pct,
            self.peak_rss_mb,
        )
    }

    /// Human summary printed to stderr.
    pub fn summary(&self) -> String {
        format!(
            "[{target}] conns={conns} payload={payload}B  \
rps={rps:.0}  p50={p50}us p99={p99}us p99.9={p999}us max={max}us  \
cpu={cpu:.1}%  rss={rss:.1}MB  ok_conns={ok}/{conns} errors={err}",
            target = self.target,
            conns = self.conns,
            payload = self.payload,
            rps = self.rps,
            p50 = self.p50_us,
            p99 = self.p99_us,
            p999 = self.p999_us,
            max = self.max_us,
            cpu = self.cpu_pct,
            rss = self.peak_rss_mb,
            ok = self.ok_conns,
            err = self.errors,
        )
    }
}