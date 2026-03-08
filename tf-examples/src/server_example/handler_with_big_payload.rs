use crate::s_type_example::{ExampleSType, ExpensiveMsg, ExpensiveResponse};
use std::net::SocketAddr;
use std::sync::Arc;
use tfserver::async_trait::async_trait;
use tfserver::codec::length_delimited::LengthDelimitedCodec;
use tfserver::server::handler::Handler;
use tfserver::structures::s_type;
use tfserver::structures::s_type::StructureType;
use tfserver::structures::traffic_proc::TrafficProcessorHolder;
use tfserver::structures::transport::Transport;
use tfserver::tokio::sync::{Mutex, RwLock};
use tfserver::tokio::sync::oneshot::Sender;
use tfserver::tokio_util::bytes::BytesMut;
use tfserver::tokio_util::codec::Framed;

pub struct BigPayloadHandler {}
#[async_trait]
impl Handler for BigPayloadHandler {
    type Codec = LengthDelimitedCodec;

    async fn serve_route(
        &mut self,
        _client_meta: (
            SocketAddr,
            &mut Option<Sender<Arc<RwLock<dyn Handler<Codec = Self::Codec>>>>>,
        ),
        s_type: Box<dyn StructureType>,
        mut data: BytesMut,
    ) -> Result<Vec<u8>, Vec<u8>> {
        match s_type.as_any().downcast_ref::<ExampleSType>().unwrap() {
            ExampleSType::ExpensiveMessage => {
                let mut message = s_type::from_slice::<ExpensiveMsg>(data.as_mut()).unwrap();
                message.data.sort();
                return Ok(s_type::to_vec(&message).unwrap());
            }
            ExampleSType::ExpensiveResponse => {

                return Ok(data.to_vec());
            }
            _ => {
                return Err("Malformed message type".into());
            }
        }
    }

    async fn accept_stream(
        &mut self,
        _add: SocketAddr,
        _stream: (
            Framed<Transport, Self::Codec>,
            TrafficProcessorHolder<Self::Codec>,
        ),
    ) {
        todo!()
    }
}
