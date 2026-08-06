use std::net::SocketAddr;
use std::sync::Arc;

use futures_util::SinkExt;
use tfserver::codec::spake2_encrypted::Spake2Encrypted;
use tfserver::server::handler::{AcceptFuture, Route, ServeFuture};
use tfserver::structures::s_type::StructureType;
use tfserver::structures::traffic_proc::TrafficProcessorHolder;
use tfserver::structures::transport::Transport;
use tfserver::tokio::sync::oneshot::Sender;
use tfserver::tokio_util::bytes::{Bytes, BytesMut};
use tfserver::tokio_util::codec::Framed;

use crate::s_type_example::ExampleSType;

/*
fn serve(
    state: &(),
    client_meta: (
        SocketAddr,
        &mut Option<Sender<Arc<Route<(), Spake2Encrypted>>>>,
    ),
    s_type: Box<dyn StructureType>,
    mut data: BytesMut,
) -> ServeFuture {
    match s_type
        .as_any()
        .downcast_ref::<ExampleSType>()
        .unwrap()
    {
        ExampleSType::ManualHandlerRequest => {
            if let Some(tx) = client_meta.1.take() {
                let _ = tx.send(route);
            }
        }
        _ => {}
    }

        Box::pin(async move {Ok(Bytes::new())})
}

fn accept_stream(
    _state: &(),
    _addr: SocketAddr,
    mut stream: (
        Framed<Transport, Spake2Encrypted>,
        TrafficProcessorHolder<Spake2Encrypted>,
    ),
) -> AcceptFuture {
    Box::pin(async move {
        stream
            .0
            .send(Bytes::from_static(b"hello from manual handler!"))
            .await
            .unwrap();
    })
}

pub fn create_route() -> Arc<Route<(), Spake2Encrypted>> {
    Arc::new(Route {
        state: Arc::new(()),
        serve,
        accept_stream: Some(accept_stream),
    })
}

 */