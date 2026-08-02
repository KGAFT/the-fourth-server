//! Tiny argument parsing + helpers shared by the bench binaries.

use std::path::PathBuf;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::time::sleep;

/// Returns the value for `--flag value` or `--flag=value`.
pub fn arg(flag: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    let prefix = format!("{flag}=");
    let mut i = 0;
    while i < args.len() {
        if args[i] == flag {
            return args.get(i + 1).cloned();
        }
        if let Some(v) = args[i].strip_prefix(&prefix) {
            return Some(v.to_string());
        }
        i += 1;
    }
    None
}

/// Parses `--flag`, falling back to `default` if missing or unparseable.
pub fn arg_or<T: std::str::FromStr>(flag: &str, default: T) -> T {
    arg(flag).and_then(|v| v.parse().ok()).unwrap_or(default)
}

/// Directory containing the current executable (where sibling bench bins live).
pub fn exe_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Polls a TCP connect until the server accepts or the timeout elapses.
pub async fn wait_for_port(addr: &str, timeout: Duration) -> bool {
    let deadline = tokio::time::Instant::now() + timeout;
    while tokio::time::Instant::now() < deadline {
        if TcpStream::connect(addr).await.is_ok() {
            return true;
        }
        sleep(Duration::from_millis(50)).await;
    }
    false
}