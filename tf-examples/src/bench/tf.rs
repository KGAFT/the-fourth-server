//! tf (the-fourth-server) echo handler, credential providers and server/client
//! builders for both the plaintext and SPAKE2-encrypted configurations.

use std::net::SocketAddr;
use std::sync::Arc;

use async_trait::async_trait;
use futures_util::future::BoxFuture;
use tfserver::codec::codec_trait::TfCodec;
use tfserver::codec::length_delimited::LengthDelimitedCodec as TfLengthDelimited;
use tfserver::codec::spake2_encrypted::{
    ClientCredentialProvider, ServerCredentialProvider, Spake2Encrypted,
};
use tfserver::server::handler::{AcceptFuture, Route, ServeFuture};
use tfserver::server::server::{ServerMode, TfServer};
use tfserver::server::server_router::TfServerRouter;
use tfserver::structures::s_type::StructureType;
use tfserver::structures::traffic_proc::TrafficProcessorHolder;
use tfserver::structures::transport::Transport;
use tfserver::tokio::sync::oneshot::Sender;
use tfserver::tokio_util::bytes::{Bytes, BytesMut};
use tfserver::tokio_util::codec::Framed;
use tfserver::tokio_util::codec::LengthDelimitedCodec as RawLengthDelimited;

use crate::bench::stype::EchoSType;

// Ensure you import the new routing types. Adjust the module path as needed.

/// Frame cap for the plaintext wrapper codec (also bounds the encrypted inner).
pub const MAX_FRAME: usize = 16 * 1024 * 1024;

pub const BENCH_PASSWORD: &[u8] = b"benchmark-shared-password";
pub const BENCH_SERVER_NAME: &str = "bench-server";
pub const BENCH_CLIENT_ID: &[u8] = b"bench-client";
pub const ECHO_HANDLER: &str = "ECHO";

pub struct BenchServerCreds;
#[async_trait]
impl ServerCredentialProvider for BenchServerCreds {
    async fn get_client_password(&self, _client_identity: &str) -> Option<Vec<u8>> {
        Some(BENCH_PASSWORD.to_vec())
    }
}

pub struct BenchClientCreds;
#[async_trait]
impl ClientCredentialProvider for BenchClientCreds {
    async fn get_client_credentials(&self) -> Option<(Vec<u8>, Vec<u8>)> {
        Some((BENCH_CLIENT_ID.to_vec(), BENCH_PASSWORD.to_vec()))
    }
}

/// Serve function for the echo route.
fn serve_echo<S: Send + Sync + 'static, C: TfCodec>(
    _state: &S,
    _client_meta: (SocketAddr, &mut Option<Sender<Arc<Route<S, C>>>>),
    _s_type: Box<dyn StructureType>,
    data: BytesMut,
) -> ServeFuture {
    Box::pin(async move { Ok(data.freeze()) })
}

/// Accept stream function for the echo route.
fn accept_echo<S: Send + Sync + 'static, C: TfCodec>(
    _state: &S,
    _addr: SocketAddr,
    _stream: (Framed<Transport, C>, TrafficProcessorHolder<C>),
) -> AcceptFuture {
    Box::pin(async move {})
}

/// Creates the echo route. We use `()` as the state since echo doesn't require any shared state.
pub fn echo_route<C: TfCodec>() -> Route<(), C> {
    Route {
        state: Arc::new(()),
        serve: serve_echo::<(), C>,
        accept_stream: Some(accept_echo::<(), C>),
    }
}

fn echo_router<C: TfCodec>() -> Arc<TfServerRouter<C, ()>> {
    let mut router: TfServerRouter<C, ()> = TfServerRouter::new(Box::new(EchoSType::Echo));

    // Note: Depending on the exact new signature of `add_route`, you might need to pass
    // `Arc::new(echo_route::<C>())` or just `echo_route::<C>()`.
    router.add_route(
        Arc::new(echo_route::<C>()),
        ECHO_HANDLER.to_string(),
        vec![Box::new(EchoSType::Echo)],
    );

    router.commit_routes();
    Arc::new(router)
}

/// Plaintext (LengthDelimitedCodec) echo server.
pub async fn build_plain_server(bind: String) -> TfServer<TfLengthDelimited, ()> {
    TfServer::new(
        bind,
        echo_router::<TfLengthDelimited>(),
        None,
        TfLengthDelimited::new(MAX_FRAME),
        None,
        ServerMode::Tcp,
    )
        .await
        .expect("bind tf plain server")
}

/// SPAKE2 + AES-256-GCM encrypted echo server.
pub async fn build_enc_server(bind: String) -> TfServer<Spake2Encrypted, ()> {
    let codec = Spake2Encrypted::create_server(
        Arc::new(BenchServerCreds),
        BENCH_SERVER_NAME.to_string(),
        RawLengthDelimited::new(),
    );
    TfServer::new(
        bind,
        echo_router::<Spake2Encrypted>(),
        None,
        codec,
        None,
        ServerMode::Tcp,
    )
        .await
        .expect("bind tf enc server")
}

/// Client codec for the plaintext server.
pub fn plain_client_codec() -> TfLengthDelimited {
    TfLengthDelimited::new(MAX_FRAME)
}

/// Client codec for the encrypted server.
pub fn enc_client_codec() -> Spake2Encrypted {
    Spake2Encrypted::create_client(
        Arc::new(BenchClientCreds),
        BENCH_SERVER_NAME.to_string(),
        RawLengthDelimited::new(),
    )
}