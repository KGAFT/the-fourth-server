use crate::server::server_router::TcpServerRouter;
use crate::structures::s_type;
use crate::structures::s_type::ServerErrorEn::InternalError;
use crate::structures::s_type::{PacketMeta, ServerErrorEn};
use std::fmt;
use std::net::SocketAddr;
use std::ops::Deref;
use std::sync::Arc;

use tokio::sync::{Mutex, Notify, RwLock};

use crate::codec::codec_trait::TfCodec;
use crate::server::handler::Handler;
use crate::structures::traffic_proc::TrafficProcessorHolder;
use crate::structures::transport::Transport;
use futures_util::SinkExt;
use tokio::io;
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::task::JoinHandle;
use tokio_rustls::TlsAcceptor;
use tokio_rustls::rustls::ServerConfig;
use tokio_util::bytes::{Bytes, BytesMut};
use tokio_util::codec::Framed;

///The request channel, used to move out tcp stream out of server control.
///
///When the stream is moved, the server does not owns it anymore.
///
///If is there need to return stream, only reconnect is available.
pub type RequestChannel<C> = (
    Sender<Arc<Mutex<dyn Handler<Codec = C>>>>,
    Receiver<Arc<Mutex<dyn Handler<Codec = C>>>>,
);


#[derive(Clone)]
pub enum ServerMode {
    /// Plain TCP or TLS
    Tcp,
    /// WebSocket upgrade over plain TCP or TLS
    WebSocket,
}

///Base binary tcp server.
///
/// 'C' is you codec, that you want to use to encode/decode data.
///
///Recommended default codec is LengthDelimitedCodec, from the server codec module.

pub struct TcpServer<C>
where
    C: TfCodec,
{
    router: Arc<TcpServerRouter<C>>,
    socket: Arc<TcpListener>,
    shutdown_sig: Arc<Notify>,
    processor: Option<TrafficProcessorHolder<C>>,
    codec: C,
    config: Option<ServerConfig>,
    mode: ServerMode,
}

impl<C> TcpServer<C>
where
    C: TfCodec,
{
    ///Creates a new instance of a server.
    ///
    /// 'bind_address' is a target address to bind current server. E.g: 0.0.0.0:8080
    /// 'router' setted up router with handlers. Must be called commit_routes before using.
    /// 'processor' Custom traffic processor, used for all streams.
    /// 'codec' basically codec used for every stream with it's own instance, when the codec is applied to stream, first call is clone, the second call is initial_setup.
    /// 'config' optional config for tls connection, when None the tls is not using, when some all connections are passed behind tls.
    pub async fn new(
        bind_address: String,
        router: Arc<TcpServerRouter<C>>,
        processor: Option<TrafficProcessorHolder<C>>,
        codec: C,
        config: Option<ServerConfig>,
        mode: ServerMode,
    ) -> Self {
        Self {
            router,
            socket: Arc::new(
                TcpListener::bind(&bind_address)
                    .await
                    .expect("Failed to bind to address"),
            ),
            shutdown_sig: Arc::new(Notify::new()),
            processor,
            codec,
            config,
            mode
        }
    }

    ///Start the task for handling connections.
    ///
    ///Return the join handle, of this task.
    pub async fn start(&mut self) -> JoinHandle<()> {
        let (listener, router, shutdown_sig) = (
            self.socket.clone(),
            self.router.clone(),
            self.shutdown_sig.clone(),
        );
        let mut processor = if let Some(proc) = self.processor.take() {
            proc
        } else {
            TrafficProcessorHolder::new()
        };
        let codec = self.codec.clone();
        let config = self.config.clone();
        let mode = self.mode.clone();   // ← new

        tokio::spawn(async move {
            loop {
                tokio::select! {
                res = listener.accept() => {
                    if let Ok((stream, addr)) = res {
                        let _ = stream.set_nodelay(true);
                        let codec = codec.clone();
                        let mode = mode.clone();    // ← new

                        // ← swapped to new unified accept
                        let transport = Self::initial_accept(stream, config.clone(), codec, &mode).await;

                        if let Some(mut transport) = transport {
                            if processor.initial_connect(&mut transport.0).await {
                                let mut framed = Framed::new(transport.0, transport.1);
                                if processor.initial_framed_connect(&mut framed).await {
                                    let router = router.clone();
                                    let prc_clone = processor.clone();
                                    tokio::spawn(async move {
                                        Self::handle_connection(addr, framed, router.as_ref(), prc_clone).await;
                                    });
                                }
                            } else {
                                let _ = transport.0.shutdown().await;
                            }
                        }
                    }
                }
                _ = shutdown_sig.notified() => break,
            }
            }
        })
    }

    ///Initial accept called for every connection, on connected event.
    async fn initial_accept(
        stream: TcpStream,
        config: Option<ServerConfig>,
        mut codec_setup: C,
        mode: &ServerMode,
    ) -> Option<(Transport, C)> {
        let mut transport = match &config {
            None => Transport::plain(stream),
            Some(cfg) => {
                let acceptor = TlsAcceptor::from(Arc::new(cfg.clone()));
                match acceptor.accept(stream).await {
                    Ok(tls) => Transport::tls_server(tls),
                    Err(_) => return None,
                }
            }
        };


        let mut transport = match mode {
            ServerMode::Tcp => transport,
            ServerMode::WebSocket => {
                match Transport::accept_websocket(transport).await {
                    Ok(ws_stream) => ws_stream,
                    Err(e) => {
                        eprintln!("WebSocket handshake failed: {e}");
                        return None;
                    }
                }
            }
        };

        if !codec_setup.initial_setup(&mut transport).await {
            return None;
        }

        Some((transport, codec_setup))
    }
    ///Stops the acceptor task.
    pub fn send_stop(&self) {
        self.shutdown_sig.notify_waiters();
    }

    ///Main function for every connection
    async fn handle_connection(
        addr: SocketAddr,
        mut stream: Framed<Transport, C>,
        router: &TcpServerRouter<C>,
        mut processor: TrafficProcessorHolder<C>,
    ) {
        use futures_util::SinkExt;
        let move_sig = tokio::sync::oneshot::channel::<Arc<RwLock<dyn Handler<Codec = C>>>>();
        let mut move_sig = (Some(move_sig.0), move_sig.1);
        loop {
            let meta_data: Result<Option<BytesMut>, bool> =
                Self::receive_message(addr.clone(), &mut stream, &mut processor).await;
            if meta_data.is_err() {
                if meta_data.unwrap_err() {
                    stream.close().await.unwrap();
                    return;
                }
                continue;
            }

            let meta_data = meta_data.unwrap();
            if meta_data.is_none() {
                continue;
            }
            let meta_data = meta_data.unwrap();
            let has_payload = match s_type::from_slice::<PacketMeta>(meta_data.deref()) {
                Ok(meta) => meta.has_payload,
                Err(_) => false,
            };

            let mut payload: BytesMut = BytesMut::new();
            if has_payload {
                let payload_res =
                    Self::receive_message(addr.clone(), &mut stream, &mut processor).await;
                if payload_res.is_err() {
                    if payload_res.unwrap_err() {
                        stream.close().await.unwrap();
                        return;
                    }
                    continue;
                }
                let payload_opt = payload_res.unwrap();
                if payload_opt.is_none() {
                    let _ = stream.close().await;
                    return;
                }
                payload = payload_opt.unwrap();
            }
            let res = router
                .serve_packet(meta_data, payload, (addr, &mut move_sig.0))
                .await;

            let message = res.unwrap_or_else(|err| s_type::to_vec(&err).unwrap());
            let res = Self::send_message(&mut stream, message, &mut processor).await;

            if let Ok(requester) = move_sig.1.try_recv() {
                requester
                    .write()
                    .await
                    .accept_stream(addr, (stream, processor.clone()))
                    .await;
                return;
            }

            match res {
                Err(_) => {
                    let _ = stream.close();
                    return;
                }
                _ => {}
            }
        }
    }
    async fn send_message(
        stream: &mut Framed<Transport, C>,
        message: Vec<u8>,
        processor: &mut TrafficProcessorHolder<C>,
    ) -> Result<(), io::Error> {
        let message = Bytes::from(processor.post_process_traffic(message).await);
        stream.send(message).await
    }

    async fn receive_message(
        _: SocketAddr,
        stream: &mut Framed<Transport, C>,
        processor: &mut TrafficProcessorHolder<C>,
    ) -> Result<Option<BytesMut>, bool> {
        use futures_util::StreamExt;
        match stream.next().await {
            Some(data) => match data {
                Ok(mut data) => {
                    data = processor.pre_process_traffic(data).await;
                    return Ok(Some(data));
                }
                Err(e) => {
                    // This is where codec-level decoding errors happen
                    match e.kind() {
                        // IO errors usually mean the connection is broken
                        std::io::ErrorKind::ConnectionReset
                        | std::io::ErrorKind::ConnectionAborted
                        | std::io::ErrorKind::BrokenPipe
                        | std::io::ErrorKind::UnexpectedEof => {
                            println!("Client disconnected");
                            return Err(true);
                        }

                        // Frame too large (if you set max_frame_length)
                        std::io::ErrorKind::InvalidData => {
                            eprintln!("Frame exceeded maximum size: {e}");
                            return Err(false);
                        }

                        // Other IO errors
                        _ => {
                            eprintln!("IO error while reading frame: {e}");
                            return Err(false);
                        }
                    }
                }
            },
            None => {
                return Err(true);
            }
        }
    }
}

// Custom Error Display
impl fmt::Display for ServerErrorEn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ServerErrorEn::MalformedMetaInfo(Some(msg)) => {
                write!(f, "Malformed meta info: {}", msg)
            }
            ServerErrorEn::MalformedMetaInfo(None) => write!(f, "Malformed meta info!"),
            ServerErrorEn::NoSuchHandler(Some(msg)) => write!(f, "No such handler: {}", msg),
            ServerErrorEn::NoSuchHandler(None) => write!(f, "No such handler!"),
            InternalError(Some(data)) => {
                write!(
                    f,
                    "{}",
                    String::from_utf8(data.clone())
                        .unwrap_or_else(|_| "Internal server error!".to_owned())
                )
            }
            InternalError(None) => {
                write!(f, "Internal server error!")
            }
            ServerErrorEn::PayloadLost => {
                write!(f, "Payload lost!")
            }
        }
    }
}

impl std::error::Error for ServerErrorEn {}
