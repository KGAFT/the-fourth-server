use std::net::SocketAddr;
use std::sync::Arc;

use tfserver::codec::spake2_encrypted::Spake2Encrypted;
use tfserver::server::handler::{AcceptFn, Route};
use tfserver::structures::s_type;
use tfserver::structures::s_type::StructureType;
use tfserver::serve;
use tfserver::tokio::sync::oneshot::Sender;
use tfserver::tokio_util::bytes::{Bytes, BytesMut};

use crate::s_type_example::{ExampleSType, TestMsg, TestResponse};

#[serve]
async fn serve_route(
    state: Arc<()>,
    addr: SocketAddr,
    route_tx: Option<Sender<(AcceptFn<(), Spake2Encrypted>, Arc<()>)>>,
    s_type: Box<dyn StructureType>,
    mut data: BytesMut,
) -> (Result<Bytes, Bytes>, Option<Sender<(AcceptFn<(), Spake2Encrypted>, Arc<()>)>>) {
    match s_type.as_any().downcast_ref::<ExampleSType>().unwrap() {
        ExampleSType::TestMessage => {
            let mut message = s_type::from_slice::<TestMsg>(data.as_mut()).unwrap();

            message.message += "Hello from server!";

            (Ok(Bytes::from_owner(s_type::to_bytes(&message).unwrap())), route_tx)
        }

        ExampleSType::TestResponse => {
            let mut message = s_type::from_slice::<TestResponse>(data.as_mut()).unwrap();

            message.another_message += "Hello from server! response";

            (Ok(Bytes::from_owner(s_type::to_bytes(&message).unwrap())), route_tx)
        }

        _ => (Err(Bytes::from_static(b"malformed message type")), route_tx),
    }
}

pub fn create_route() -> Arc<Route<(), Spake2Encrypted>> {
    Arc::new(Route {
        state: Arc::new(()),
        serve: serve_route,
        //accept_stream: None, // no stream takeover
    })
}
