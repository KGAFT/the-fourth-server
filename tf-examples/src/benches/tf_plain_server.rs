//! Plaintext (LengthDelimitedCodec) the-fourth-server echo server.
//! Usage: bench_tf_plain_server --port 19001

use tf_examples::bench::{default_port, tf, util};

#[tokio::main]
async fn main() {
    let port: u16 = util::arg_or("--port", default_port("tf_plain"));
    let mut server = tf::build_plain_server(format!("0.0.0.0:{port}")).await;
    let handle = server.start().await;
    eprintln!("bench_tf_plain_server listening on 0.0.0.0:{port}");
    let _ = handle.await;
}