use tfserver::server::handler::{AcceptFn, ServeFuture};

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
    state: Arc<()>,
    addr: SocketAddr,
    mut route_tx: Option<Sender<(AcceptFn<(), Spake2Encrypted>, Arc<()>)>>,
    structure: Box<dyn StructureType>,
    mut bytes: BytesMut,
) -> (Result<Bytes, Bytes>, Option<Sender<(AcceptFn<(), Spake2Encrypted>, Arc<()>)>>){

        match structure
            .as_any()
            .downcast_ref::<ExampleSType>()
            .unwrap()
        {
            ExampleSType::ExpensiveMessage => {
                let mut message =
                    s_type::from_slice::<ExpensiveMsg>(bytes.as_mut()).unwrap();

                message.data.sort();
                

                (Ok(Bytes::from_owner(
                    s_type::to_bytes(&message).unwrap(),
                )), route_tx)
            }

            ExampleSType::ExpensiveResponse => {
                /*
                if let Some(req) = route_tx.take() {
                    let _ = req.send((accept_stream, state));
                }

                 */

                (Ok(bytes.freeze()), route_tx)
            }
            

            _ => (Err(Bytes::from_static(b"Malformed message type")), route_tx),
        }
    }

#[accept]
async fn accept_stream(
    state: Arc<()>,
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
    })
}
