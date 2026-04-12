pub mod target_router;

use crate::client::target_router::TargetRouter;
use crate::structures::s_type;
use crate::structures::s_type::{PacketMeta, StructureType, SystemSType};
use crate::structures::traffic_proc::TrafficProcessorHolder;
use crate::structures::transport::Transport;
use futures_util::SinkExt;
use std::io;
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::sync::{mpsc};
use tokio_rustls::TlsConnector;
use tokio_rustls::rustls::ClientConfig;
use tokio_util::bytes::{Bytes, BytesMut};
use tokio_util::codec::{ Framed};
use crate::codec::codec_trait::TfCodec;

#[derive(Debug)]
pub enum ClientError {
    Io(io::Error),
    Tls(String),
    Codec(io::Error),
    Router(String),
    ChannelClosed,
    Protocol(String),
}

impl From<io::Error> for ClientError {
    fn from(e: io::Error) -> Self {
        ClientError::Io(e)
    }
}

pub struct ClientConnect {
    tx: Sender<ClientRequest>,
}

#[derive( Clone)]
///The structure that describes target handler
pub struct HandlerInfo {
    id: Option<u64>,
    named: Option<String>,
}

impl HandlerInfo {
    ///Creates handler info by handler name
    pub fn new_named(name: String) -> Self {
        Self {
            id: None,
            named: Some(name),
        }
    }
    ///Creates handler info by handler id
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
/// 'handler_info' info about target handler
/// 'data' the request payload. E.g structure that will be deserialized on server side.
/// 's_type' structure type indetifiers what data is send and how handler on server side will process this data.
pub struct DataRequest {
    pub handler_info: HandlerInfo,
    pub data: Vec<u8>,
    pub s_type: Box<dyn StructureType>,
}
///The request wrapper struct.
/// 'req' data request
/// 'consumer' the signal that will be called by connection, when the response arrives
 
pub struct ClientRequest {
    pub req: DataRequest,
    pub consumer: tokio::sync::oneshot::Sender<BytesMut>,
}

impl ClientConnect {
    ///Creates and connect to the designated server address
    /// 'server_name' used for tls mode. You need to pass domain name of the server. If there is no tls, you can pass random data or empty
    /// 'connection_dest' the (server address/domain name):port. E.g temp_domain.com:443, or 65.88.95.127:9090.
    /// 'processor' the traffic processor, must be symmetric to the server one processor.
    /// 'codec' the connection codec. Recommended base LengthDelimitedCodec from module codec.
    /// 'client_config' the tls config.
    /// 'max_request_in_time' max amount of requests that can be dispatched in the same time.
    pub async fn new<
        C: TfCodec,
    >(
        server_name: String,
        connection_dest: String,
        processor: Option<TrafficProcessorHolder<C>>,
        mut codec: C,
        client_config: Option<ClientConfig>,
        max_request_in_time: usize,
    ) -> Result<Self, ClientError> {
        let socket = TcpStream::connect(connection_dest).await?;
        socket.set_nodelay(true)?;

        let mut transport = if let Some(client_config) = client_config {
            let connector = TlsConnector::from(Arc::new(client_config));
            let domain = server_name
                .try_into()
                .map_err(|_| ClientError::Tls("Invalid server name".into()))?;

            let tls = connector
                .connect(domain, socket)
                .await
                .map_err(|e| ClientError::Tls(e.to_string()))?;

            Transport::tls_client(tls)
        } else {
            Transport::plain(socket)
        };
        if !codec.initial_setup(&mut transport).await {
            panic!("Failed to initial setup transport");
        }
        let framed = Framed::new(transport, codec);
        let (tx, rx) = mpsc::channel(max_request_in_time);

        Self::connection_main(framed, processor, rx);

        Ok(Self { tx })
    }

    ///Dispatches the request.
    pub async fn dispatch_request(&self, request: ClientRequest) -> Result<(), ClientError> {
        self.tx
            .send(request)
            .await
            .map_err(|_| ClientError::ChannelClosed)
    }
    
    fn connection_main<
        C: TfCodec,
    >(
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
                    eprintln!("Client request failed: {:?}", err);
                }
            }
        });
    }

    async fn process_request<
        C: TfCodec,
    >(
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

        let meta_vec = s_type::to_vec(&meta)
            .ok_or_else(|| ClientError::Protocol("PacketMeta serialization failed".into()))?;

        let meta_bytes = processor.post_process_traffic(meta_vec).await;
        let payload = processor.post_process_traffic(request.req.data).await;

        socket.send(Bytes::from(meta_bytes)).await?;
        socket.send(Bytes::from(payload)).await?;

        let response = wait_for_data(socket).await?;
        let response = processor.pre_process_traffic(response).await;

        let _ = request
            .consumer
            .send(response);

        Ok(())
    }
}

pub async fn wait_for_data<
    C: TfCodec,
>(
    socket: &mut Framed<Transport, C>,
) -> Result<BytesMut, ClientError> {
    use futures_util::StreamExt;

    match socket.next().await {
        Some(Ok(data)) => Ok(data),
        Some(Err(e)) => Err(ClientError::Codec(e)),
        None => Err(ClientError::Protocol("Connection closed".into())),
    }
}
