use crate::client::wait_for_data;
use crate::structures::s_type;
use crate::structures::s_type::{HandlerMetaAns, HandlerMetaReq, SystemSType};
use crate::structures::traffic_proc::TrafficProcessorHolder;
use crate::structures::transport::Transport;
use futures_util::SinkExt;
use std::collections::HashMap;
use std::io;
use tokio_util::codec::{Framed};
use crate::codec::codec_trait::TfCodec;

#[derive(Debug)]
pub enum RouterError {
    Io(io::Error),
    Protocol(String),
    Codec(String),
    ConnectionClosed,
}

impl From<io::Error> for RouterError {
    fn from(e: io::Error) -> Self {
        RouterError::Io(e)
    }
}

///The structures that handler router_name/router_id mapping requests
pub struct TargetRouter {
    known_routes: HashMap<String, u64>,
}

impl TargetRouter {
    pub fn new() -> Self {
        Self {
            known_routes: HashMap::new(),
        }
    }
    ///Returns the id of handler, if the route was already requested from server
    pub fn lookup_route(&self, route: &str) -> Option<u64> {
        self.known_routes.get(route).cloned()
    }
    ///Returns the id of handler, but may request id from server side.
    pub async fn request_route<
        C: TfCodec,
    >(
        &mut self,
        route: &str,
        stream: &mut Framed<Transport, C>,
        processor: &mut TrafficProcessorHolder<C>,
    ) -> Result<u64, RouterError> {
        if let Some(id) = self.lookup_route(route) {
            return Ok(id);
        }

        let id = Self::request_route_from_server(route, stream, processor).await?;
        self.known_routes.insert(route.to_string(), id);
        Ok(id)
    }

    async fn request_route_from_server<
        C: TfCodec,
    >(
        name: &str,
        stream: &mut Framed<Transport, C>,
        processor: &mut TrafficProcessorHolder<C>,
    ) -> Result<u64, RouterError> {
        let meta_req = HandlerMetaReq {
            s_type: SystemSType::HandlerMetaReq,
            handler_name: name.to_string(),
        };

        let serialized = s_type::to_vec(&meta_req)
            .ok_or(RouterError::Protocol("serialization failed".into()))?;

        let processed = processor.post_process_traffic(serialized).await;

        stream.send(processed.into()).await?;

        let response = wait_for_data(stream)
            .await
            .map_err(|_| RouterError::ConnectionClosed)?;

        let mut response = processor.pre_process_traffic(response).await;

        let meta_ans = s_type::from_slice::<HandlerMetaAns>(response.as_mut())
            .map_err(|e| RouterError::Protocol(e.to_string()))?;

        Ok(meta_ans.id)
    }
}
