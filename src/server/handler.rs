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

pub type ServeFuture = BoxFuture<'static, Result<Bytes, Bytes>>;

pub type AcceptFuture = BoxFuture<'static, ()>;

pub type ServeFn<S, C> = fn(
    &S,
    (SocketAddr, &mut Option<Sender<Arc<Route<S, C>>>>),
    Box<dyn StructureType>,
    BytesMut,
) -> ServeFuture;

pub type AcceptFn<S, C> =
    fn(&S, SocketAddr, (Framed<Transport, C>, TrafficProcessorHolder<C>)) -> AcceptFuture;

pub struct Route<S, C>
where
    S: Send + Sync + 'static,
    C: TfCodec,
{
    pub state: Arc<S>,

    pub serve: ServeFn<S, C>,

    pub accept_stream: Option<AcceptFn<S, C>>,
}
