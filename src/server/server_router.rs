use std::collections::{HashMap, HashSet};
use std::net::{SocketAddr};
use std::ops::Deref;
use std::panic::AssertUnwindSafe;
use std::sync::{Arc};
use bytes::Bytes;
use futures_util::FutureExt;
use rkyv::util::AlignedVec;
use tokio::sync::{RwLock};
use tokio::sync::oneshot::Sender;
use tokio_util::bytes::{BytesMut};
use crate::codec::codec_trait::TfCodec;
use crate::server::handler::{Route};
use crate::structures::s_type;
use crate::structures::s_type::{HandlerMetaAns, HandlerMetaReq, PacketMeta, ServerError, ServerErrorEn, StructureType, SystemSType, TypeContainer, TypeTuple};
use crate::structures::s_type::ServerErrorEn::InternalError;


///Tcp server router.
///Handles the every data route destination
pub struct TfServerRouter<C, S>
where
    C: TfCodec, S: Send + Sync + 'static, {
    //@TODO get rid of dyn dispatch and RwLock, also replace hashmap with dashmap, maybe?
    routes: Arc<HashMap<TypeTuple, Arc<Route<S, C>>>>,
    routes_text_names: Arc<HashMap<String, u64>>,
    routes_to_add: Vec<(TypeTuple, ( Arc<Route<S, C>>, String))>,
    router_incremental: u64,
    routes_commited: bool,
    user_s_type: Box<dyn StructureType>,
}

impl<C, S> TfServerRouter<C, S>
where
    C: TfCodec, S: Send + Sync + 'static, {
    
    ///Returns the new instance of router
    ///
    /// 'user_s_type' random enum value of current project defined structure_type
    pub fn new(user_s_type: Box<dyn StructureType>) -> Self {
        Self {
            routes: Arc::new(HashMap::new()),
            routes_text_names: Arc::new(HashMap::new()),
            routes_to_add: Vec::new(),
            router_incremental: 0,
            routes_commited: false,
            user_s_type,
        }
    }

    ///Registers the new handler, for selected structure types
    ///
    /// 'handler_name' must be the same on the client site, used for initial identification. When client sends request to server.
    /// 's_type' handled structure types by current handler.
    pub fn add_route(
        &mut self,
        route:  Arc<Route<S, C>>,
        handler_name: String,
        mut s_types: Vec<Box<dyn StructureType>>,
    ) {
        if self.routes_commited {
            return;
        }
        let mut s_typess: HashSet<TypeContainer> = HashSet::new();
        while !s_types.is_empty(){
            s_typess.insert(TypeContainer::new(s_types.pop().unwrap()));
        }
        let types_tupple = TypeTuple {
            s_types: s_typess,
            handler_id: self.router_incremental,
        };

        self.routes_to_add.push((types_tupple, (route, handler_name)));
        self.router_incremental += 1;
    }

    ///Commits the registered handlers. Making the current router is final and ready to be passed to the server
    pub fn commit_routes(&mut self) {
        if self.routes_commited || self.routes_to_add.is_empty() {
            return;
        }

        let mut routes = HashMap::new();
        let mut names = HashMap::new();

        for (types, (handler, name)) in self.routes_to_add.drain(..) {
            routes.insert(types.clone(), handler);
            names.insert(name, types.handler_id);
        }

        self.routes = Arc::new(routes);
        self.routes_text_names = Arc::new(names);
        self.routes_commited = true;
    }


    pub fn get_routes(&self) -> Arc<HashMap<TypeTuple, Arc<Route<S, C>>>> {
        self.routes.clone()
    }

    ///Called from server connection task
    pub async fn serve_packet(
        &self,
        meta: BytesMut,
        payload: BytesMut,
        client_meta: (SocketAddr,  &mut Option<Sender<Arc<Route<S, C>>>>),
    ) -> Result<Bytes, ServerError> {
        // Try to deserialize normal PacketMeta
        if let Ok(meta_pack) = s_type::access::<PacketMeta>(&meta) {
            let s_type = self.user_s_type.get_deserialize_function().deref()(meta_pack.s_type_req.to_native());
            let key = TypeTuple {
                s_types: HashSet::from([TypeContainer::new(s_type.clone_unique())]),
                handler_id: meta_pack.handler_id.to_native(),
            };

            let handler = self.routes.get(&key).ok_or(ServerError::new(ServerErrorEn::NoSuchHandler(None)))?;

            let fut =
                (handler.serve)(handler.state.as_ref(), client_meta, s_type, payload);

            let res = AssertUnwindSafe(fut)
                .catch_unwind()
                .await;

            return match res {
                Ok(data) => match data{
                    Ok(data) => Ok(data),
                    Err(err) => {Err(ServerError::new(ServerErrorEn::InternalError(Some(Vec::from(err)))))}
                },
                Err(_) => Err(ServerError::new(InternalError(Some("handler died :(".as_bytes().to_vec())))),
            };
        }

        // Try to handle as HandlerMetaReq
        if let Ok(meta_req) = s_type::access::<HandlerMetaReq>(&meta) {
            if let Some(route_id) = self.routes_text_names.get(&meta_req.handler_name.to_string()) {
                let meta_ans = HandlerMetaAns {
                    s_type: SystemSType::HandlerMetaAns,
                    id: *route_id,
                };
                return Ok(Bytes::from_owner(s_type::to_bytes(&meta_ans).unwrap()));
            } else {
                return Err(ServerError::new(ServerErrorEn::NoSuchHandler(None)));
            }
        }

        Err(ServerError::new(ServerErrorEn::MalformedMetaInfo(None)))
    }
}
