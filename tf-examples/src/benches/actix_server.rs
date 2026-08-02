//! actix-web HTTP echo server. POST /echo returns the request body unchanged.
//! Usage: bench_actix_server --port 19004

use actix_web::{App, HttpResponse, HttpServer, web};
use tf_examples::bench::{default_port, util};

async fn echo(body: web::Bytes) -> HttpResponse {
    HttpResponse::Ok().body(body)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let port: u16 = util::arg_or("--port", default_port("actix"));
    eprintln!("bench_actix_server listening on 0.0.0.0:{port}");
    HttpServer::new(|| App::new().route("/echo", web::post().to(echo)))
        .bind(("0.0.0.0", port))?
        .run()
        .await
}