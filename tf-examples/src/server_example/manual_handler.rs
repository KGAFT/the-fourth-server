use crate::s_type_example::{ExampleSType};
use std::net::SocketAddr;
use std::sync::{Arc};
use tfserver::async_trait::async_trait;
use tfserver::codec::length_delimited::LengthDelimitedCodec;
use tfserver::futures_util::SinkExt;
use tfserver::server::handler::Handler;
use tfserver::structures::s_type::StructureType;
use tfserver::structures::traffic_proc::TrafficProcessorHolder;
use tfserver::structures::transport::Transport;
use tfserver::tokio::sync::{Mutex, RwLock};
use tfserver::tokio::sync::oneshot::Sender;
use tfserver::tokio_util::bytes::BytesMut;
use tfserver::tokio_util::codec::Framed;

pub struct ManualHandler {
    pub self_ref: Option<Arc<RwLock<ManualHandler>>>,
}

#[async_trait]
impl Handler for ManualHandler {
    type Codec = LengthDelimitedCodec;

    async fn serve_route(
        &mut self,
        client_meta: (
            SocketAddr,
            &mut Option<Sender<Arc<RwLock<dyn Handler<Codec = Self::Codec>>>>>,
        ),
        s_type: Box<dyn StructureType>,
        data: BytesMut,
    ) -> Result<Vec<u8>, Vec<u8>> {
        match s_type.as_any().downcast_ref::<ExampleSType>().unwrap() {
            ExampleSType::ManualHandlerRequest => {
                if let Some(cli) = client_meta.1.take() {
                    let _ = cli.send(self.self_ref.as_ref().unwrap().clone());
                }
                Ok(data.to_vec())
            }
            _ => {
                Err("Invalid type in ExampleSType".into())
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
        stream
            .0
            .send("hello from manual handler!".as_bytes().into())
            .await
            .unwrap();
    }
}
