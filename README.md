# the-fourth-server

A lightweight, async binary TCP server/client library for Rust built on [Tokio](https://tokio.rs/).

Packets are framed and serialized with [bincode](https://github.com/bincode-org/bincode), routed to typed handlers on the server, and optionally encrypted end-to-end. Both plain TCP and WebSocket transports are supported, and the client side compiles to **WASM**.

[![Crates.io](https://img.shields.io/crates/v/the-fourth-server)](https://crates.io/crates/the-fourth-server)
[![docs.rs](https://img.shields.io/docsrs/the-fourth-server)](https://docs.rs/the-fourth-server)
[![License](https://img.shields.io/crates/l/the-fourth-server)](LICENSE)

---

## Features

- **Binary framing** — length-delimited frames via `tokio-util`
- **Typed routing** — requests are dispatched to handlers by a user-defined structure-type enum
- **Pluggable codec** — implement `TfCodec` to add custom framing or per-connection setup (e.g. encryption handshake)
- **SPAKE2 + AES-256-GCM encryption** — built-in password-authenticated key exchange with replay protection
- **Transport flexibility** — plain TCP, TLS (`tokio-rustls`), or WebSocket upgrade
- **WASM client** — the client compiles to `wasm32` using WebSocket transport
- **Stream handoff** — a handler can take ownership of the raw stream for protocol upgrades or proxying
- **Optional logging** — enable the `logging` feature to get structured logs via the `log` facade

---

## Transports

| Mode | Server | Client (native) | Client (WASM) |
|------|--------|-----------------|---------------|
| Plain TCP (`tcp://`) | yes | yes | no |
| TLS (`tls://`) | yes | yes | no |
| WebSocket (`ws://`) | yes | yes | yes |
| WebSocket over TLS (`wss://`) | yes | yes | yes |

TLS and WebSocket are composed at the `Transport` layer, so passing a `ServerConfig` together with `ServerMode::WebSocket` gives you `wss://` with no extra wiring. The client uses `wss://` in the URL and the underlying library handles TLS automatically.

---

## Getting started

Add the dependency:

```toml
[dependencies]
the-fourth-server = "0.3"
```

Enable logging (optional):

```toml
[dependencies]
the-fourth-server = { version = "0.3", features = ["logging"] }
```

---

## Concepts

### Structure types

Every message carries a *structure type* tag — a user-defined enum that implements the `StructureType` trait. The router uses this tag (plus the handler ID) to dispatch the packet to the correct handler.

```rust
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum MyType {
    Request,
    Response,
}

// implement StructureType for MyType (see tf-examples for a full example)
```

### Codec

`TfCodec` extends `tokio_util::codec::{Encoder, Decoder}` with an `initial_setup` async method called once per new connection. This is where per-connection negotiation (e.g. the SPAKE2 handshake) happens. You can write your own codec if you need it

Two codecs are provided:

| Codec | Description |
|-------|-------------|
| `LengthDelimitedCodec` | Plain framing from `tokio-util` — no encryption |
| `Spake2Encrypted` | SPAKE2 handshake → HKDF → AES-256-GCM with replay protection |

### Server

```rust
use std::sync::Arc;
use tfserver::codec::spake2_encrypted::Spake2Encrypted;
use tfserver::server::server::{TfServer, ServerMode};
use tfserver::server::server_router::TfServerRouter;
use tfserver::tokio_util::codec::LengthDelimitedCodec;

// 1. Build the router from your #[serve]-annotated routes
let mut router: TfServerRouter<Spake2Encrypted, ()> =
    TfServerRouter::new(Box::new(MyType::Response));

router.add_route(
    route.clone(), // Arc<Route<(), Spake2Encrypted>>
    "MY_HANDLER".to_string(),
    vec![
        Box::new(MyType::Request),
        Box::new(MyType::Response),
    ],
);

router.commit_routes();

let router = Arc::new(router);

// 2. Start the server
let mut server = TfServer::new(
    "0.0.0.0:9000".to_string(),
    router,
    None, // optional TrafficProcessor
    Spake2Encrypted::create_server(
        Arc::new(MyServerCredProvider {}),
        "server".to_string(),
        LengthDelimitedCodec::new(),
    ),
    None, // optional TLS ServerConfig
    ServerMode::Tcp,
)
.await?;

server.start().await;
```


### Handlers


```rust
use tfserver::{serve, accept};
use tfserver::structures::s_type;

#[serve]
async fn serve_route(
    state: &MyState,
    addr: SocketAddr,
    route_tx: &mut Option<Sender<Arc<Route<MyState, LengthDelimitedCodec>>>>,
    s_type: Box<dyn StructureType>,
    data: BytesMut,
) -> Result<Bytes, Bytes> {
    let req: MyRequest = s_type::from_slice(&data).unwrap();
    let resp = MyResponse { /* ... */ };
    Ok(Bytes::from(s_type::to_vec(&resp).unwrap()))
}

#[accept]
async fn accept_stream(
    state: &MyState,
    addr: SocketAddr,
    mut framed: Framed<Transport, LengthDelimitedCodec>,
    holder: TrafficProcessorHolder<LengthDelimitedCodec>,
) {
    // called only if this route requests stream ownership
}
```

Argument order and shapes are fixed:

| Macro | Signature |
|-------|-----------|
| `#[serve]` | `(state: &S, addr: SocketAddr, route_tx: &mut Option<Sender<Arc<Route<S, C>>>>, s_type: Box<dyn StructureType>, data: BytesMut) -> Result<Bytes, Bytes>` |
| `#[accept]` | `(state: &S, addr: SocketAddr, framed: Framed<Transport, C>, holder: TrafficProcessorHolder<C>) -> ()` |

Each expands to a plain `fn` — no closures, no captured state — so it coerces
directly to `ServeFn<S, C>` / `AcceptFn<S, C>` and can be registered on a `Route`:

```rust
let route = Arc::new(Route {
    state: Arc::new(MyState::default()),
    serve: serve_route,             // fn pointer, from #[serve]
    accept_stream: Some(accept_stream), // fn pointer, from #[accept]; None if unused
});
```

### Client

```rust
use tfserver::client::{ClientConnect, ClientMode, ClientRequest, DataRequest, HandlerInfo};
use tfserver::structures::s_type;
use tfserver::tokio_util::codec::LengthDelimitedCodec;

let client = ClientConnect::new(
    "localhost".to_string(),
    "127.0.0.1:9000".to_string(),
    None,                         // optional TrafficProcessor
    LengthDelimitedCodec::new(),
    ClientMode::Tcp { client_config: None },
    64,                           // max in-flight requests
).await?;

let (tx, rx) = tokio::sync::oneshot::channel();
client.dispatch_request(ClientRequest {
    req: DataRequest {
        handler_info: HandlerInfo::new_named("MY_HANDLER".to_string()),
        data: s_type::to_vec(&my_request).unwrap(),
        s_type: Box::new(MyType::Request),
    },
    consumer: tx,
}).await?;

let response_bytes = rx.await.unwrap();
```

---

## Encrypted transport (SPAKE2 + AES-256-GCM)

`Spake2Encrypted` performs a password-authenticated key exchange before any application data is sent. No pre-shared keys or PKI are required — just a shared password per client identity.

**Server side:**

```rust
use tfserver::codec::spake2_encrypted::{Spake2Encrypted, ServerCredentialProvider};

struct MyCredProvider;

#[async_trait]
impl ServerCredentialProvider for MyCredProvider {
    async fn get_client_password(&self, client_identity: &str) -> Option<Vec<u8>> {
        // look up password for the connecting client
        Some(b"s3cr3t".to_vec())
    }
}

let codec = Spake2Encrypted::create_server(
    Arc::new(MyCredProvider),
    "my-server".to_string(),
    LengthDelimitedCodec::new(),
);
```

**Client side:**

```rust
use tfserver::codec::spake2_encrypted::{Spake2Encrypted, ClientCredentialProvider};

struct MyClientCreds;

#[async_trait]
impl ClientCredentialProvider for MyClientCreds {
    async fn get_client_credentials(&self) -> Option<(Vec<u8>, Vec<u8>)> {
        Some((b"alice".to_vec(), b"s3cr3t".to_vec())) // (identity, password)
    }
}

let codec = Spake2Encrypted::create_client(
    Arc::new(MyClientCreds),
    "my-server".to_string(),
    LengthDelimitedCodec::new(),
);
```

After the handshake, all frames are encrypted with AES-256-GCM. Each frame carries an 8-byte monotonic counter used as the nonce; replayed or reordered packets are rejected.

---

## WebSocket transport

Pass `ServerMode::WebSocket` / `ClientMode::WebSocket` to upgrade the TCP connection to WebSocket before any application framing. This is the only mode available in WASM builds.

```rust
// server
TfServer::new("0.0.0.0:9000".to_string(), router, None, codec, None, ServerMode::WebSocket).await?;

// client (native or WASM)
ClientConnect::new(
    String::new(),
    String::new(),
    None,
    codec,
    ClientMode::WebSocket { url: "ws://127.0.0.1:9000/ws".to_string() },
    64,
).await?;
```

---

## Examples

Working examples are in the `tf-examples/` workspace member:

| File | Description |
|------|-------------|
| `server_ex.rs` | WebSocket server with SPAKE2 encryption and three handlers |
| `client_ex.rs` | Client sending test and large-payload requests |
| `s_type_ex.rs` | Example structure-type enum definition |

---

