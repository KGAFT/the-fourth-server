use crate::log_macros::{tf_debug, tf_error, tf_info, tf_warn};
use crate::server::server_router::TfServerRouter;
use crate::structures::s_type;
use crate::structures::s_type::{PacketMeta, ServerErrorEn};
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

/// The request channel, used to move out tcp stream out of server control.
///
/// When the stream is moved, the server does not own it anymore.
///
/// If there is a need to return the stream, only reconnect is available.
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

/// Base binary TCP server.
///
/// `C` is the codec used to encode/decode data.
///
/// Recommended default codec is `LengthDelimitedCodec` from the server codec module.
pub struct TfServer<C>
where
    C: TfCodec,
{
    router: Arc<TfServerRouter<C>>,
    socket: Arc<TcpListener>,
    shutdown_sig: Arc<Notify>,
    processor: Option<TrafficProcessorHolder<C>>,
    codec: C,
    config: Option<ServerConfig>,
    mode: ServerMode,
}

impl<C> TfServer<C>
where
    C: TfCodec,
{
    /// Creates a new server instance bound to `bind_address`.
    ///
    /// Returns an error if the address cannot be bound.
    pub async fn new(
        bind_address: String,
        router: Arc<TfServerRouter<C>>,
        processor: Option<TrafficProcessorHolder<C>>,
        codec: C,
        config: Option<ServerConfig>,
        mode: ServerMode,
    ) -> Result<Self, io::Error> {
        let socket = TcpListener::bind(&bind_address).await.map_err(|e| {
            tf_error!("Failed to bind to {}: {}", bind_address, e);
            e
        })?;
        tf_info!("Server bound to {}", bind_address);
        Ok(Self {
            router,
            socket: Arc::new(socket),
            shutdown_sig: Arc::new(Notify::new()),
            processor,
            codec,
            config,
            mode,
        })
    }

    /// Start the task for handling connections.
    ///
    /// Returns the join handle for the acceptor task.
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
        let mode = self.mode.clone();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    res = listener.accept() => {
                        match res {
                            Ok((stream, addr)) => {
                                tf_debug!("Accepted connection from {}", addr);
                                let _ = stream.set_nodelay(true);
                                let codec = codec.clone();
                                let mode = mode.clone();
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
                                        } else {
                                            tf_warn!("Framed processor rejected connection from {}", addr);
                                        }
                                    } else {
                                        tf_warn!("Processor rejected connection from {}", addr);
                                        let _ = transport.0.shutdown().await;
                                    }
                                }
                            }
                            Err(e) => {
                                tf_warn!("Accept error: {}", e);
                            }
                        }
                    }
                    _ = shutdown_sig.notified() => {
                        tf_info!("Server shutting down");
                        break;
                    }
                }
            }
        })
    }

    async fn initial_accept(
        stream: TcpStream,
        config: Option<ServerConfig>,
        mut codec_setup: C,
        mode: &ServerMode,
    ) -> Option<(Transport, C)> {
        let transport = match &config {
            None => Transport::plain(stream),
            Some(cfg) => {
                let acceptor = TlsAcceptor::from(Arc::new(cfg.clone()));
                match acceptor.accept(stream).await {
                    Ok(tls) => Transport::tls_server(tls),
                    Err(e) => {
                        tf_warn!("TLS handshake failed: {}", e);
                        return None;
                    }
                }
            }
        };

        let mut transport = match mode {
            ServerMode::Tcp => transport,
            ServerMode::WebSocket => match Transport::accept_websocket(transport).await {
                Ok(ws_stream) => ws_stream,
                Err(e) => {
                    tf_warn!("WebSocket handshake failed: {}", e);
                    return None;
                }
            },
        };

        if !codec_setup.initial_setup(&mut transport).await {
            tf_warn!("Codec initial_setup rejected connection");
            return None;
        }

        Some((transport, codec_setup))
    }

    /// Signals the acceptor task to stop.
    pub fn send_stop(&self) {
        self.shutdown_sig.notify_waiters();
    }

    async fn handle_connection(
        addr: SocketAddr,
        mut stream: Framed<Transport, C>,
        router: &TfServerRouter<C>,
        mut processor: TrafficProcessorHolder<C>,
    ) {
        use futures_util::SinkExt;
        let move_sig = tokio::sync::oneshot::channel::<Arc<RwLock<dyn Handler<Codec = C>>>>();
        let mut move_sig = (Some(move_sig.0), move_sig.1);
        loop {
            let meta_data: Result<Option<BytesMut>, bool> =
                Self::receive_message(addr, &mut stream, &mut processor).await;
            if meta_data.is_err() {
                if meta_data.unwrap_err() {
                    stream.close().await.unwrap_or_else(|e| {
                        tf_warn!("Error closing stream for {}: {}", addr, e);
                    });
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
                Err(e) => {
                    tf_warn!("Failed to deserialize PacketMeta from {}: {}", addr, e);
                    false
                }
            };

            let mut payload: BytesMut = BytesMut::new();
            if has_payload {
                let payload_res =
                    Self::receive_message(addr, &mut stream, &mut processor).await;
                if payload_res.is_err() {
                    if payload_res.unwrap_err() {
                        stream.close().await.unwrap_or_else(|e| {
                            tf_warn!("Error closing stream for {}: {}", addr, e);
                        });
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

            if let Err(e) = res {
                tf_warn!("Send error for {}: {}", addr, e);
                let _ = stream.close();
                return;
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
        addr: SocketAddr,
        stream: &mut Framed<Transport, C>,
        processor: &mut TrafficProcessorHolder<C>,
    ) -> Result<Option<BytesMut>, bool> {
        use futures_util::StreamExt;
        match stream.next().await {
            Some(data) => match data {
                Ok(mut data) => {
                    data = processor.pre_process_traffic(data).await;
                    Ok(Some(data))
                }
                Err(e) => match e.kind() {
                    std::io::ErrorKind::ConnectionReset
                    | std::io::ErrorKind::ConnectionAborted
                    | std::io::ErrorKind::BrokenPipe
                    | std::io::ErrorKind::UnexpectedEof => {
                        tf_debug!("Client {} disconnected", addr);
                        Err(true)
                    }
                    std::io::ErrorKind::InvalidData => {
                        tf_warn!("Frame exceeded maximum size from {}: {}", addr, e);
                        Err(false)
                    }
                    _ => {
                        tf_warn!("IO error reading frame from {}: {}", addr, e);
                        Err(false)
                    }
                },
            },
            None => Err(true),
        }
    }
}
