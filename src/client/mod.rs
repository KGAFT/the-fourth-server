pub mod target_router;

use crate::client::target_router::TargetRouter;
use crate::log_macros::{tf_debug, tf_info, tf_warn};
use crate::structures::s_type;
use crate::structures::s_type::{PacketMeta, StructureType, SystemSType};
use crate::structures::traffic_proc::TrafficProcessorHolder;
use crate::structures::transport::Transport;
use futures_util::SinkExt;
use std::io;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::Arc;
use rkyv::util::AlignedVec;
#[cfg(not(target_arch = "wasm32"))]
use tokio::net::TcpStream;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::sync::mpsc;
#[cfg(not(target_arch = "wasm32"))]
use tokio_rustls::TlsConnector;
#[cfg(not(target_arch = "wasm32"))]
use tokio_rustls::rustls::ClientConfig;
use tokio_util::bytes::{Bytes, BytesMut};
use tokio_util::codec::Framed;
use crate::codec::codec_trait::TfCodec;

#[derive(Clone)]
pub enum ClientMode {
    /// Raw TCP, optionally wrapped in TLS
    #[cfg(not(target_arch = "wasm32"))]
    Tcp { client_config: Option<ClientConfig> },
    /// WebSocket — for environments without raw TCP access (e.g. WASM)
    /// `url` is the full ws:// or wss:// URL, e.g. "wss://example.com:9000/ws"
    WebSocket { url: String },
}

#[derive(Debug)]
pub enum ClientError {
    Io(io::Error),
    Tls(String),
    Codec(io::Error),
    Router(String),
    ChannelClosed,
    Protocol(String),
    /// Codec `initial_setup` failed during connection establishment.
    Setup(String),
}

impl From<io::Error> for ClientError {
    fn from(e: io::Error) -> Self {
        ClientError::Io(e)
    }
}

pub struct ClientConnect {
    tx: Sender<ClientRequest>,
}

#[derive(Clone)]
/// Describes the target handler on the server.
pub struct HandlerInfo {
    id: Option<u64>,
    named: Option<String>,
}

impl HandlerInfo {
    /// Creates handler info by handler name.
    pub fn new_named(name: String) -> Self {
        Self {
            id: None,
            named: Some(name),
        }
    }
    /// Creates handler info by handler id.
    pub fn new_id(id: u64) -> Self {
        Self {
            id: Some(id),
            named: None,
        }
    }

    pub fn id(&self) -> Option<u64> {
        self.id
    }

    pub fn named(&self) -> &Option<String> {
        &self.named
    }
}

/// `handler_info` — info about the target handler.
/// `data` — the request payload (serialized structure).
/// `s_type` — identifies what data is sent and how the server handler processes it.
pub struct DataRequest {
    pub handler_info: HandlerInfo,
    pub data: Bytes,
    pub s_type: Box<dyn StructureType>,
}

/// Wraps a data request and the channel that receives the server response.
pub struct ClientRequest {
    pub req: DataRequest,
    pub consumer: tokio::sync::oneshot::Sender<BytesMut>,
}

impl ClientConnect {
    /// Creates a client and connects to the server.
    ///
    /// - `server_name`: used for TLS SNI; may be empty when not using TLS.
    /// - `connection_dest`: `host:port`, e.g. `"65.88.95.127:9090"`.
    /// - `max_request_in_time`: capacity of the in-flight request channel.
    pub async fn new<C: TfCodec>(
        server_name: String,
        connection_dest: String,
        processor: Option<TrafficProcessorHolder<C>>,
        mut codec: C,
        mode: ClientMode,
        max_request_in_time: usize,
    ) -> Result<Self, ClientError> {
        tf_info!("Connecting to {}", connection_dest);
        let mut transport = Self::connect(server_name, connection_dest.clone(), &mode).await?;

        if !codec.initial_setup(&mut transport).await {
            tf_warn!("Codec initial_setup failed for {}", connection_dest);
            return Err(ClientError::Setup(
                "codec initial_setup failed".into(),
            ));
        }
        tf_debug!("Connected to {}", connection_dest);

        let framed = Framed::new(transport, codec);
        let (tx, rx) = mpsc::channel(max_request_in_time);
        Self::connection_main(framed, processor, rx);

        Ok(Self { tx })
    }

    async fn connect(
        server_name: String,
        connection_dest: String,
        mode: &ClientMode,
    ) -> Result<Transport, ClientError> {
        match mode {
            #[cfg(not(target_arch = "wasm32"))]
            ClientMode::Tcp { client_config } => {
                let socket = TcpStream::connect(&connection_dest).await?;
                socket.set_nodelay(true)?;

                if let Some(cfg) = client_config {
                    let connector = TlsConnector::from(Arc::new(cfg.clone()));
                    let domain = server_name
                        .try_into()
                        .map_err(|_| ClientError::Tls("Invalid server name".into()))?;
                    let tls = connector
                        .connect(domain, socket)
                        .await
                        .map_err(|e| ClientError::Tls(e.to_string()))?;
                    Ok(Transport::tls_client(tls))
                } else {
                    Ok(Transport::plain(socket))
                }
            }

            ClientMode::WebSocket { url } => {
                Transport::connect(url).await.map_err(|e| ClientError::Tls(e.to_string()))
            }
        }
    }

    /// Dispatches a request to the server.
    pub async fn dispatch_request(&self, request: ClientRequest) -> Result<(), ClientError> {
        self.tx
            .send(request)
            .await
            .map_err(|_| ClientError::ChannelClosed)
    }

    fn connection_main<C: TfCodec>(
        mut socket: Framed<Transport, C>,
        processor: Option<TrafficProcessorHolder<C>>,
        mut rx: Receiver<ClientRequest>,
    ) {
        let mut processor = processor.unwrap_or_else(TrafficProcessorHolder::new);
        let mut router = TargetRouter::new();

        tokio::spawn(async move {
            while let Some(request) = rx.recv().await {
                if let Err(err) =
                    Self::process_request(request, &mut socket, &mut processor, &mut router).await
                {
                    tf_warn!("Client request failed: {:?}", err);
                }
            }
            tf_debug!("Client connection loop ended");
        });
    }

    async fn process_request<C: TfCodec>(
        request: ClientRequest,
        socket: &mut Framed<Transport, C>,
        processor: &mut TrafficProcessorHolder<C>,
        target_router: &mut TargetRouter,
    ) -> Result<(), ClientError> {
        let handler_id = match request.req.handler_info.id() {
            Some(id) => id,
            None => {
                let name = request
                    .req
                    .handler_info
                    .named
                    .ok_or_else(|| ClientError::Protocol("Missing handler name".into()))?;

                tf_debug!("Requesting route id for handler '{}'", name);
                target_router
                    .request_route(name.as_str(), socket, processor)
                    .await
                    .map_err(|e| ClientError::Router(format!("{:?}", e)))?
            }
        };

        let meta = PacketMeta {
            s_type: SystemSType::PacketMeta,
            s_type_req: request.req.s_type.get_serialize_function()(request.req.s_type),
            handler_id,
            has_payload: !request.req.data.is_empty(),
        };

        let meta_vec = Bytes::from_owner( s_type::to_bytes(&meta)
            .ok_or_else(|| ClientError::Protocol("PacketMeta serialization failed".into()))?);

        let meta_bytes = processor.post_process_traffic(meta_vec).await;
        let payload = processor.post_process_traffic(request.req.data).await;

        socket.send(Bytes::from_owner(meta_bytes)).await?;
        socket.send(Bytes::from_owner(payload)).await?;

        let response = wait_for_data(socket).await?;
        let response = processor.pre_process_traffic(response).await;

        let _ = request.consumer.send(response);

        Ok(())
    }
}

pub async fn wait_for_data<C: TfCodec>(
    socket: &mut Framed<Transport, C>,
) -> Result<BytesMut, ClientError> {
    use futures_util::StreamExt;

    match socket.next().await {
        Some(Ok(data)) => Ok(data),
        Some(Err(e)) => Err(ClientError::Codec(e)),
        None => Err(ClientError::Protocol("Connection closed".into())),
    }
}