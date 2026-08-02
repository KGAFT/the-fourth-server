//! axum HTTP echo server. POST /echo returns the request body unchanged.
//! Usage: bench_axum_server --port 19003

use axum::{Router, body::Bytes, routing::post};
use tf_examples::bench::{default_port, util};

async fn echo(body: Bytes) -> Bytes {
    body
}

#[tokio::main]
async fn main() {
    let port: u16 = util::arg_or("--port", default_port("axum"));
    let app = Router::new().route("/echo", post(echo));
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port))
        .await
        .expect("bind axum");
    eprintln!("bench_axum_server listening on 0.0.0.0:{port}");
    axum::serve(listener, app).await.expect("axum serve");
}