use std::net::SocketAddr;
use std::sync::Arc;
use bytes::Bytes;
use tfserver::async_trait::async_trait;
use tfserver::codec::length_delimited::LengthDelimitedCodec;
use tfserver::codec::spake2_encrypted::Spake2Encrypted;
use tfserver::server::handler::Handler;
use tfserver::structures::s_type;
use tfserver::structures::s_type::StructureType;
use tfserver::structures::traffic_proc::TrafficProcessorHolder;
use tfserver::structures::transport::Transport;
use tfserver::tokio::sync::{Mutex, RwLock};
use tfserver::tokio::sync::oneshot::Sender;
use tfserver::tokio_util::bytes::BytesMut;
use tfserver::tokio_util::codec::Framed;
use crate::s_type_example::{ExampleSType, TestMsg, TestResponse};

pub struct TestHandler {}
#[async_trait]
impl Handler for TestHandler {
    type Codec = Spake2Encrypted;

    async fn serve_route(
        &mut self,
        _client_meta: (
            SocketAddr,
            &mut Option<Sender<Arc<RwLock<dyn Handler<Codec = Self::Codec>>>>>,
        ),
        s_type: Box<dyn StructureType>,
        mut data: BytesMut,
    ) -> Result<Bytes, Bytes> {
        match s_type.as_any().downcast_ref::<ExampleSType>().unwrap(){
            ExampleSType::TestMessage => {
                let mut message = s_type::from_slice::<TestMsg>(data.as_mut()).unwrap();
                message.message+="Hello from server!";
                return Ok(Bytes::from_owner(s_type::to_bytes(&message).unwrap()));
            }
            ExampleSType::TestResponse => {
                let mut message = s_type::from_slice::<TestResponse>(data.as_mut()).unwrap();
                message.another_message+="Hello from server! response";
                return Ok(Bytes::from_owner(s_type::to_bytes(&message).unwrap()));
            }
            _=> {
                Err("malformed message type".into())
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
