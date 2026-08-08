use crate::codec::codec_trait::TfCodec;
use crate::structures::s_type::StructureType;
use crate::structures::traffic_proc::TrafficProcessorHolder;
use crate::structures::transport::Transport;
use bytes::Bytes;
use futures_util::future::BoxFuture;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::oneshot::Sender;
use tokio_util::bytes::BytesMut;
use tokio_util::codec::Framed;

pub type ServeFuture<S, C> = BoxFuture<
'static,
(Result<Bytes, Bytes>, Option<Sender<(AcceptFn<S, C>, Arc<S>)>>),
>;
pub type AcceptFuture = BoxFuture<'static, ()>;

///Your route serve function, accepts appstate, client_meta(addr_info and request for querrying move current stream for manual handling)


pub type ServeFn<S, C> = fn(
    Arc<S>,
    SocketAddr,
    Option<Sender<(AcceptFn<S, C>, Arc<S>)>>,
    Box<dyn StructureType>,
    BytesMut,
) -> ServeFuture<S, C>;

pub type AcceptFn<S, C> =
    fn(Arc<S>, SocketAddr, (Framed<Transport, C>, TrafficProcessorHolder<C>)) -> AcceptFuture;

///'S' - global AppState structure per project.
pub struct Route<S, C>
where
    S: Send + Sync + 'static,
    C: TfCodec,
{
    pub state: Arc<S>,

    pub serve: ServeFn<S, C>,
}
