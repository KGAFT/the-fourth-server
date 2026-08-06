use std::net::SocketAddr;
use std::sync::Arc;

use bytes::Bytes;
use tfserver::codec::spake2_encrypted::Spake2Encrypted;
use tfserver::server::handler::{AcceptFuture, Route, ServeFuture};
use tfserver::structures::s_type;
use tfserver::structures::s_type::StructureType;
use tfserver::structures::traffic_proc::TrafficProcessorHolder;
use tfserver::structures::transport::Transport;
use tfserver::tokio::sync::oneshot::Sender;
use tfserver::tokio_util::bytes::BytesMut;
use tfserver::tokio_util::codec::Framed;

use crate::s_type_example::{ExampleSType, TestMsg, TestResponse};


fn serve(
    _state: &(),
    _client_meta: (
        SocketAddr,
        &mut Option<Sender<Arc<Route<(), Spake2Encrypted>>>>,
    ),
    s_type: Box<dyn StructureType>,
    mut data: BytesMut,
) -> ServeFuture {
    Box::pin(async move {
        match s_type.as_any().downcast_ref::<ExampleSType>().unwrap() {
            ExampleSType::TestMessage => {
                let mut message =
                    s_type::from_slice::<TestMsg>(data.as_mut()).unwrap();

                message.message += "Hello from server!";

                Ok(Bytes::from_owner(
                    s_type::to_bytes(&message).unwrap(),
                ))
            }

            ExampleSType::TestResponse => {
                let mut message =
                    s_type::from_slice::<TestResponse>(data.as_mut()).unwrap();

                message.another_message += "Hello from server! response";

                Ok(Bytes::from_owner(
                    s_type::to_bytes(&message).unwrap(),
                ))
            }

            _ => Err(Bytes::from_static(b"malformed message type")),
        }
    })
}

fn accept_stream(
    _state: &(),
    _addr: SocketAddr,
    _stream: (
        Framed<Transport, Spake2Encrypted>,
        TrafficProcessorHolder<Spake2Encrypted>,
    ),
) -> AcceptFuture {
    Box::pin(async move {
        // This handler never accepts streams.
    })
}

pub fn create_route() -> Arc<Route<(), Spake2Encrypted>> {
    Arc::new(Route {
        state: Arc::new(()),
        serve,
        accept_stream: None, // no stream takeover
    })
}