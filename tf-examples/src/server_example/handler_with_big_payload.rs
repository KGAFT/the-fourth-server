use std::net::SocketAddr;
use std::sync::Arc;

use futures_util::{future::BoxFuture, SinkExt};
use tfserver::codec::spake2_encrypted::Spake2Encrypted;
use tfserver::server::handler::{Route, ServeFuture, AcceptFuture};
use tfserver::structures::s_type;
use tfserver::structures::s_type::StructureType;
use tfserver::structures::traffic_proc::TrafficProcessorHolder;
use tfserver::structures::transport::Transport;
use tfserver::tokio::sync::oneshot::Sender;
use tfserver::tokio_util::bytes::{Bytes, BytesMut};
use tfserver::tokio_util::codec::Framed;

use crate::s_type_example::{ExampleSType, ExpensiveMsg};

pub struct BigPayloadState;

fn serve(
    _state: &BigPayloadState,
    _client_meta: (
        SocketAddr,
        &mut Option<Sender<Arc<Route<BigPayloadState, Spake2Encrypted>>>>,
    ),
    s_type: Box<dyn StructureType>,
    mut data: BytesMut,
) -> ServeFuture {
    Box::pin(async move {
        match s_type
            .as_any()
            .downcast_ref::<ExampleSType>()
            .unwrap()
        {
            ExampleSType::ExpensiveMessage => {
                let mut message =
                    s_type::from_slice::<ExpensiveMsg>(data.as_slice()).unwrap();

                message.data.sort();

                Ok(Bytes::from_owner(
                    s_type::to_bytes(&message).unwrap(),
                ))
            }

            ExampleSType::ExpensiveResponse => {
                Ok(data.freeze())
            }

            _ => Err(Bytes::from_static(b"Malformed message type")),
        }
    })
}

fn accept_stream(
    _state: &BigPayloadState,
    _addr: SocketAddr,
    mut stream: (
        Framed<Transport, Spake2Encrypted>,
        TrafficProcessorHolder<Spake2Encrypted>,
    ),
) -> AcceptFuture {
    Box::pin(async move {
        println!("accepted stream, now we break the connection");

        stream
            .0
            .send(Bytes::from_static(b"This will break"))
            .await
            .unwrap();

        stream.0.close().await.unwrap();
    })
}

pub fn create_route() -> Arc<Route<BigPayloadState, Spake2Encrypted>> {
    Arc::new(Route {
        state: Arc::new(BigPayloadState),

        serve,

        accept_stream: Some(accept_stream),
    })
}