use std::net::SocketAddr;
use std::sync::{Arc};
use async_trait::async_trait;
use tokio::sync::{RwLock};
use tokio::sync::oneshot::Sender;
use tokio_util::bytes::{BytesMut};
use tokio_util::codec::{Framed};
use crate::codec::codec_trait::TfCodec;
use crate::structures::s_type::StructureType;
use crate::structures::traffic_proc::TrafficProcessorHolder;
use crate::structures::transport::Transport;

#[async_trait]
///The server handler trait. Used for handling data from client/
pub trait Handler: Send + Sync {
    type Codec:  TfCodec;
    ///The serve_route called by router, when the new data arrived and designated to registered to this handler structure_types.
    ///
    /// 'client_meta' is client info, and signal for requesting to move stream.
    ///If request needed, call take() on this option
    ///'''if let Some(tx) = meta.1.take(){
    ///'''            tx.send(...).unwrap();
    ///'''}
    /// 's_type' is identified request structure type
    /// 'data' is binary representation of the structure. Call the deserialize from s_type to turn it into base structure.
    async fn serve_route(
        &mut self,
        /*If request needed, call take() on this option
        *if let Some(tx) = meta.1.take(){
        *            tx.send(...).unwrap();
        *        }
        */
        client_meta: (SocketAddr,  &mut Option<Sender<Arc<RwLock<dyn Handler<Codec = Self::Codec>>>>>),
        s_type: Box<dyn StructureType>,
        data: BytesMut,
    ) -> Result<Vec<u8>, Vec<u8>>;

    ///This function called, when server received the request of handler to move stream. It returns all needed data for this stream.
    async fn accept_stream(&mut self, add: SocketAddr, stream: (Framed<Transport, Self::Codec>, TrafficProcessorHolder<Self::Codec>));

}
