//! lynn_tcp echo server. Echoes received bytes back to the sending client.
//! Usage: bench_lynn_server --port 19005

use lynn_tcp::{lynn_server::*, lynn_tcp_dependents::*};
use tf_examples::bench::{ECHO_METHOD_ID, default_port, util};

// Borrowed by the config builder (&'a {usize,bool}), so keep them 'static.
const MAX_CONNS: usize = 200_000;
// lynn's default reactor taskpool is 512; keep the default to avoid
// over-spawning spinning reactor workers.
const TASKPOOL: usize = 512;
// lynn defaults TCP_NODELAY off, which adds ~40ms (Nagle + delayed ACK) to a
// ping-pong workload. Enable it so the comparison with tf/axum/actix is fair.
const TCP_NODELAY: bool = true;
// All load originates from 127.0.0.1, so the per-IP cap and connection rate
// limit must be lifted or most connections get rejected.
const MAX_PER_IP: usize = 200_000;
const RATE_LIMIT: u64 = 10_000_000;

async fn echo(input: InputBufVO) -> HandlerResult {
    match input.get_input_addr() {
        Some(addr) => HandlerResult::new_with_send(ECHO_METHOD_ID, input.get_all_bytes().freeze(), vec![addr]),
        None => HandlerResult::new_without_send(),
    }
}

#[tokio::main]
async fn main() {
    let port: u16 = util::arg_or("--port", default_port("lynn"));
    let config = LynnServerConfigBuilder::new()
        .with_addr(format!("0.0.0.0:{port}"))
        .expect("lynn addr")
        .with_server_max_connections(Some(&MAX_CONNS))
        .with_server_max_taskpool_size(&TASKPOOL)
        .with_tcp_nodelay(&TCP_NODELAY)
        .with_max_connections_per_ip(&MAX_PER_IP)
        .with_connection_rate_limit(&RATE_LIMIT)
        .build();
    eprintln!("bench_lynn_server listening on 0.0.0.0:{port}");
    let _ = LynnServer::new_with_config(config)
        .await
        .add_router(ECHO_METHOD_ID, echo)
        .start()
        .await;
}