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
async fn serve_route(
    state: Arc<()>,
    addr: SocketAddr,
    route_tx: &mut Option<Sender<(AcceptFn<(), Spake2Encrypted>, Arc<()>)>>,
    structure: Box<dyn StructureType>,
    mut bytes: BytesMut,
) -> Result<Bytes, Bytes> {
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

    Ok(Bytes::new())
}

#[accept]
async fn accept_stream(
    state: &(),
    addr: SocketAddr,
    mut framed: Framed<Transport, Spake2Encrypted>,
    holder: TrafficProcessorHolder<Spake2Encrypted>)
{

        println!("accepted stream, now we break the connection");

        framed
            .send(Bytes::from_static(b"This will break"))
            .await
            .unwrap();

        framed.close().await.unwrap();
    }


pub fn create_route() -> Arc<Route<(), Spake2Encrypted>> {
    Arc::new(Route {
        state: Arc::new(()),
        serve,
    })
}

 */