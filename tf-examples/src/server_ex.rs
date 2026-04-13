
mod s_type_example;
mod server_example;

use std::sync::Arc;
use tfserver::codec::length_delimited::LengthDelimitedCodec;
use tfserver::server::server::ServerMode::{Tcp, WebSocket};
use tfserver::server::server_router::TcpServerRouter;
use tfserver::server::server::TcpServer;
use tfserver::tokio;
use tfserver::tokio::sync::{Mutex, RwLock};
use crate::s_type_example::ExampleSType;
use crate::server_example::handler_with_big_payload::BigPayloadHandler;
use crate::server_example::manual_handler::ManualHandler;
use crate::server_example::test_handler::TestHandler;

#[tokio::main]
pub async fn main(){
    let test_handler = TestHandler{};
    let big_payload_handler = BigPayloadHandler{};
    let manual_handler = Arc::new(RwLock::new(ManualHandler{self_ref: None}));
    manual_handler.write().await.self_ref = Some(manual_handler.clone());

    let mut router: TcpServerRouter<LengthDelimitedCodec> = TcpServerRouter::new(Box::new(ExampleSType::TestResponse));
    router.add_route(Arc::new(RwLock::new(test_handler)), "TEST_HANDLER".to_string(), vec![Box::new(ExampleSType::TestMessage), Box::new(ExampleSType::TestResponse)]);
    router.add_route(Arc::new(RwLock::new(big_payload_handler)), "BIG_PAYLOAD".to_string(), vec![Box::new(ExampleSType::ExpensiveMessage), Box::new(ExampleSType::ExpensiveResponse)]);
    router.add_route(manual_handler, "MANUAL_HANDLER".to_string(), vec![Box::new(ExampleSType::ManualHandlerRequest)]);

    router.commit_routes();
    let router = Arc::new(router);

    let mut server = TcpServer::new("0.0.0.0:9973".to_string(), router, None, LengthDelimitedCodec::new(1024 * 1024 * 1024), None, WebSocket).await;
    server.start().await;
    tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
    server.send_stop();

}