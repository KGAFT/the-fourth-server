//! SPAKE2 + AES-256-GCM encrypted the-fourth-server echo server.
//! Usage: bench_tf_enc_server --port 19002

use tf_examples::bench::{default_port, tf, util};

#[tokio::main]
async fn main() {
    let port: u16 = util::arg_or("--port", default_port("tf_enc"));
    let mut server = tf::build_enc_server(format!("0.0.0.0:{port}")).await;
    let handle = server.start().await;
    eprintln!("bench_tf_enc_server listening on 0.0.0.0:{port}");
    let _ = handle.await;
}