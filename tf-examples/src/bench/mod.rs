//! Shared benchmark plumbing: latency stats, CPU/RSS sampling, the echo
//! protocol used by the tf servers, and small CLI helpers.

pub mod proc;
pub mod stats;
pub mod stype;
pub mod tf;
pub mod util;

/// Method id used by every echo server (lynn routes on this, the others ignore it).
pub const ECHO_METHOD_ID: u16 = 1;

/// Default listen ports per target so the runner and servers agree without wiring.
pub fn default_port(target: &str) -> u16 {
    match target {
        "tf_plain" => 19001,
        "tf_enc" => 19002,
        "axum" => 19003,
        "actix" => 19004,
        "lynn" => 19005,
        _ => 19000,
    }
}

/// Maps a target name to the compiled server binary that the runner spawns.
pub fn server_bin(target: &str) -> Option<&'static str> {
    Some(match target {
        "tf_plain" => "bench_tf_plain_server",
        "tf_enc" => "bench_tf_enc_server",
        "axum" => "bench_axum_server",
        "actix" => "bench_actix_server",
        "lynn" => "bench_lynn_server",
        _ => return None,
    })
}