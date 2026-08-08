extern crate core;

mod s_type_example;
mod server_example;

use crate::s_type_example::ExampleSType;

use std::sync::Arc;
use tfserver::async_trait::async_trait;
use tfserver::codec::spake2_encrypted::{ServerCredentialProvider, Spake2Encrypted};
use tfserver::server::server::ServerMode::{Tcp, WebSocket};
use tfserver::server::server::TfServer;
use tfserver::server::server_router::TfServerRouter;
use tfserver::tokio;
use tfserver::tokio::sync::{Mutex, RwLock};
use tfserver::tokio_util::codec::{LengthDelimitedCodec, LengthDelimitedCodecError};
use crate::server_example::{handler_with_big_payload, test_handler};

pub struct TestServerCredProvider {}
#[async_trait]
impl ServerCredentialProvider for TestServerCredProvider {
    async fn get_client_password(&self, client_identity: &str) -> Option<Vec<u8>> {
        Some("HelloPasswordForHandshake".as_bytes().to_vec())
    }
}
#[tokio::main]
pub async fn main() {
    env_logger::init();

    let mut router: TfServerRouter<Spake2Encrypted, ()> =
        TfServerRouter::new(Box::new(ExampleSType::TestResponse));
    router.add_route(
        test_handler::create_route(),
        "TEST_HANDLER".to_string(),
        vec![
            Box::new(ExampleSType::TestMessage),
            Box::new(ExampleSType::TestResponse),
        ],
    );
    router.add_route(
       handler_with_big_payload::create_route(),
        "BIG_PAYLOAD".to_string(),
        vec![
            Box::new(ExampleSType::ExpensiveMessage),
            Box::new(ExampleSType::ExpensiveResponse),
        ],
    );


    router.commit_routes();
    let router = Arc::new(router);

    let mut server = TfServer::new(
        "0.0.0.0:9973".to_string(),
        router,
        None,
        
        Spake2Encrypted::create_server(
            Arc::new(TestServerCredProvider {}),
            "server".to_string(),
            LengthDelimitedCodec::new(),
        ),
        None,
        Tcp,
    )
    .await
    .expect("Failed to create server");
    server.start().await;
    tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
    server.send_stop();
}
