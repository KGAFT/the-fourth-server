use tfserver::server::handler::ServeFuture;

use tfserver::server::handler::AcceptFuture;
use std::net::SocketAddr;
use std::sync::Arc;
use futures_util::SinkExt;
use tokio::sync::oneshot::Sender;
use tfserver::codec::spake2_encrypted::Spake2Encrypted;
use tfserver::server::handler::{Route};
use tfserver::structures::s_type;
use tfserver::structures::s_type::StructureType;
use tfserver::structures::traffic_proc::TrafficProcessorHolder;
use tfserver::structures::transport::Transport;
use tfserver::{accept, serve};
use tfserver::tokio_util::bytes::{Bytes, BytesMut};
use tfserver::tokio_util::codec::Framed;
use crate::s_type_example::{ExampleSType, ExpensiveMsg};

#[serve]
async fn serve_route(
    state: &(),
    addr: SocketAddr,
    route_tx: &mut Option<Sender<Arc<Route<(), Spake2Encrypted>>>>,
    structure: Box<dyn StructureType>,
    bytes: BytesMut,
) -> Result<Bytes, Bytes> {

        match structure
            .as_any()
            .downcast_ref::<ExampleSType>()
            .unwrap()
        {
            ExampleSType::ExpensiveMessage => {
                let mut message =
                    s_type::from_slice::<ExpensiveMsg>(bytes.as_slice()).unwrap();

                message.data.sort();
                
                
                Ok(Bytes::from_owner(
                    s_type::to_bytes(&message).unwrap(),
                ))
            }

            ExampleSType::ExpensiveResponse => {
                Ok(bytes.freeze())
            }
            

            _ => Err(Bytes::from_static(b"Malformed message type")),
        }
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

        serve: serve_route,

        accept_stream: Some(accept_stream),
    })
}
