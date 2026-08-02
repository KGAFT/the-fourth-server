//! Linux /proc-based CPU% and peak-RSS sampling for the server child process.
//!
//! CPU% is derived from the delta of utime+stime jiffies over wall time and can
//! exceed 100% on multi-core machines. Peak RSS is read from `VmHWM`, which the
//! kernel already tracks as the high-water mark, so no periodic polling needed.

use std::time::Instant;

/// Standard Linux scheduler tick. `getconf CLK_TCK` is 100 on essentially all
/// modern x86/ARM Linux builds; documented as an assumption in the README.
const CLK_TCK: u64 = 100;

pub struct CpuMemSampler {
    pid: u32,
    start_jiffies: u64,
    start: Instant,
}

impl CpuMemSampler {
    pub fn start(pid: u32) -> Self {
        Self {
            pid,
            start_jiffies: read_jiffies(pid).unwrap_or(0),
            start: Instant::now(),
        }
    }

    /// Returns (cpu_percent, peak_rss_mb). Call just before killing the child.
    pub fn finish(&self) -> (f64, f64) {
        let end_jiffies = read_jiffies(self.pid).unwrap_or(self.start_jiffies);
        let secs = self.start.elapsed().as_secs_f64().max(1e-6);
        let cpu_pct =
            (end_jiffies.saturating_sub(self.start_jiffies) as f64 / CLK_TCK as f64) / secs * 100.0;
        let peak_rss_mb = read_vmhwm_kb(self.pid).unwrap_or(0) as f64 / 1024.0;
        (cpu_pct, peak_rss_mb)
    }
}

/// utime + stime in clock ticks from /proc/<pid>/stat.
fn read_jiffies(pid: u32) -> Option<u64> {
    let s = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // comm (field 2) is wrapped in parens and may itself contain spaces/parens,
    // so split after the final ')'. The token stream then starts at `state`.
    let close = s.rfind(')')?;
    let rest = s.get(close + 2..)?;
    let fields: Vec<&str> = rest.split_whitespace().collect();
    // state=index0 (field3) ... utime=field14=index11, stime=field15=index12
    let utime: u64 = fields.get(11)?.parse().ok()?;
    let stime: u64 = fields.get(12)?.parse().ok()?;
    Some(utime + stime)
}

/// Peak resident set size in kB from /proc/<pid>/status (VmHWM).
fn read_vmhwm_kb(pid: u32) -> Option<u64> {
    let s = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    for line in s.lines() {
        if let Some(rest) = line.strip_prefix("VmHWM:") {
            return rest.split_whitespace().next()?.parse().ok();
        }
    }
    None
}