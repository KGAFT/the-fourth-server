use crate::s_type_example::{ExampleSType, ExpensiveMsg, ExpensiveResponse, TestMsg};
use rand::{Rng, random};
use std::time::{Duration, Instant};
use tfserver::client::{ClientConnect, ClientMode, ClientRequest, DataRequest, HandlerInfo};
use tfserver::codec::length_delimited::LengthDelimitedCodec;
use tfserver::structures::s_type;
use tfserver::tokio;
use tfserver::tokio::time::sleep;

mod s_type_example;
///Basically just serializing request
fn make_test_request() -> DataRequest {
    let request = TestMsg {
        s_type: ExampleSType::TestMessage,
        id: 12,
        data: vec![32, 32, 23, 42],
        message: "hello from client".to_string(),
    };
    let data_req = DataRequest {
        handler_info: HandlerInfo::new_named("TEST_HANDLER".to_string()),
        data: s_type::to_vec(&request).unwrap(),
        s_type: Box::new(ExampleSType::TestMessage),
    };
    data_req
}

fn make_big_payload_request() -> DataRequest {
    let mut rng = rand::rng();
    let size = 8096; // 8 kilobytes
    let request = ExpensiveResponse {
        s_type: ExampleSType::ExpensiveResponse,
        id: 12,
        data: (0..size).map(|_| rng.r#gen()).collect(),
    };
    let data_req = DataRequest {
        handler_info: HandlerInfo::new_named("BIG_PAYLOAD".to_string()),
        data: s_type::to_vec(&request).unwrap(),
        s_type: Box::new(ExampleSType::ExpensiveResponse),
    };
    data_req
}

fn make_very_big_payload_request() -> DataRequest {
    let mut rng = rand::rng();
    let size = 250 * 1024 * 1024; // 250 MB
    let request = ExpensiveResponse {
        s_type: ExampleSType::ExpensiveResponse,
        id: 12,
        data: (0..size).map(|_| rng.r#gen()).collect(),
    };
    let data_req = DataRequest {
        handler_info: HandlerInfo::new_named("BIG_PAYLOAD".to_string()),
        data: s_type::to_vec(&request).unwrap(),
        s_type: Box::new(ExampleSType::ExpensiveResponse),
    };
    data_req
}

#[tokio::main]
///The client is simple as hell.
///Just create ClientConnect with same as server params.
/// Each request from client, new oneshot channel to receive response
async fn main() {
    let mut client_connect = ClientConnect::new(
        "localhost".to_string(),
        "127.0.0.1:9973".to_string(),
        None,
        LengthDelimitedCodec::new(1024 * 1024 * 1024),
       // ClientMode::WebSocket {url: "ws://127.0.0.1:9973/ws".into()  },
        ClientMode::Tcp {client_config: None},
        2500,

    )
    .await
    .expect("Connecting to server failed");
    loop {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let test_req = make_test_request();
        let client_req = ClientRequest {
            req: test_req,
            consumer: tx,
        };
        let start = Instant::now();
        client_connect
            .dispatch_request(client_req)
            .await
            .expect("Sending request failed");
        if let Ok(resp) = rx.await {
            println!("Delay {} microseconds", start.elapsed().as_micros());
        }

        let (tx, rx) = tokio::sync::oneshot::channel();
        let test_req = make_big_payload_request();
        let client_req = ClientRequest {
            req: test_req,
            consumer: tx,
        };
        let start = Instant::now();
        client_connect
            .dispatch_request(client_req)
            .await
            .expect("Sending request failed");
        if let Ok(mut resp) = rx.await {
            println!(
                "Delay on high payload request: {} microseconds",
                start.elapsed().as_micros()
            );
            let resp: ExpensiveResponse = s_type::from_slice(resp.as_mut()).unwrap();
            //   println!("Received response: {:?}", resp);
        }
        /*
        let (tx, rx) = tokio::sync::oneshot::channel();
        let test_req = make_very_big_payload_request();
        let client_req = ClientRequest{ req: test_req, consumer: tx};
        let start = Instant::now();
        client_connect.dispatch_request(client_req).await.expect("Sending request failed");
        if let Ok(mut resp) = rx.await {
            println!("Delay on high payload request: {} microseconds", start.elapsed().as_micros());
            let resp: ExpensiveResponse = s_type::from_slice(resp.as_mut()).unwrap();
            //   println!("Received response: {:?}", resp);
        }

         */
    }
}
