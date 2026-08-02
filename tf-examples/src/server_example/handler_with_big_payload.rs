use crate::s_type_example::{ExampleSType, ExpensiveMsg, ExpensiveResponse};
use std::net::SocketAddr;
use std::sync::Arc;
use tfserver::async_trait::async_trait;
use tfserver::codec::length_delimited::LengthDelimitedCodec;
use tfserver::codec::spake2_encrypted::Spake2Encrypted;
use tfserver::futures_util::SinkExt;
use tfserver::server::handler::Handler;
use tfserver::structures::s_type;
use tfserver::structures::s_type::StructureType;
use tfserver::structures::traffic_proc::TrafficProcessorHolder;
use tfserver::structures::transport::Transport;
use tfserver::tokio::sync::{Mutex, RwLock};
use tfserver::tokio::sync::oneshot::Sender;
use tfserver::tokio_util::bytes::{Bytes, BytesMut};
use tfserver::tokio_util::codec::Framed;

pub struct BigPayloadHandler {
    pub(crate) self_ref: Option<Arc<RwLock<Self>>>,
}
#[async_trait]
impl Handler for BigPayloadHandler {
    type Codec = Spake2Encrypted;

    async fn serve_route(
        &mut self,
        client_meta: (
            SocketAddr,
            &mut Option<Sender<Arc<RwLock<dyn Handler<Codec = Self::Codec>>>>>,
        ),
        s_type: Box<dyn StructureType>,
        mut data: BytesMut,
    ) -> Result<Bytes, Bytes> {
        match s_type.as_any().downcast_ref::<ExampleSType>().unwrap() {
            ExampleSType::ExpensiveMessage => {
                let mut message = s_type::from_slice::<ExpensiveMsg>(data.as_mut()).unwrap();
                message.data.sort();
                return Ok(Bytes::from_owner(s_type::to_bytes(&message).unwrap()));
            }
            ExampleSType::ExpensiveResponse => {
                return Ok(data.freeze());
            }
            _ => {
                return Err("Malformed message type".into());
            }
        }
    }

    async fn accept_stream(
        &mut self,
        _add: SocketAddr,
        mut stream: (
            Framed<Transport, Self::Codec>,
            TrafficProcessorHolder<Self::Codec>,
        ),
    ) {
        println!("accepted stream, now we break the connection");
        stream.0.send("This will break".as_bytes().into()).await.unwrap();
        stream.0.close().await.unwrap();
    }
}
