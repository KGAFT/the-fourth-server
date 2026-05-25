use std::io;
use std::io::Error;
use crate::codec::codec_trait::TfCodec;
use crate::structures::temp_transport::TempTransport;
use crate::structures::transport::{AsyncReadWrite, Transport};
use aes_gcm::{
    Aes256Gcm, Key, Nonce,
    aead::{Aead, AeadCore, KeyInit, OsRng},
};
use async_trait::async_trait;
use bytes::{Bytes, BytesMut};
use futures_util::{SinkExt, StreamExt};
use hkdf::Hkdf;
use sha2::Sha256;
use spake2::{Ed25519Group, Identity, Password, Spake2};
use std::sync::Arc;
use aead::AeadInPlace;
use tokio_util::codec::{Decoder, Encoder, Framed, LengthDelimitedCodec};

pub struct Spake2Encrypted {
    server_provider: Option<Arc<dyn ServerCredentialProvider>>,
    client_provider: Option<Arc<dyn ClientCredentialProvider>>,
    is_server: bool,
    server_id: Vec<u8>,
    length_codec: LengthDelimitedCodec,
    keys: Option<SessionKeys>,
}

impl Spake2Encrypted {
    pub fn create_server(
        server_provider: Arc<dyn ServerCredentialProvider>,
        server_id: String,
        codec: LengthDelimitedCodec,
    ) -> Self {
        Self {
            server_provider: Some(server_provider),
            client_provider: None,
            is_server: true,
            server_id: server_id.as_bytes().to_vec(),
            length_codec: codec,
            keys: None,
        }
    }

    pub fn create_client(
        client_provider: Arc<dyn ClientCredentialProvider>,
        server_id: String,
        codec: LengthDelimitedCodec,
    ) -> Self {
        Self {
            server_provider: None,
            client_provider: Some(client_provider),
            is_server: false,
            server_id: server_id.as_bytes().to_vec(),
            length_codec: codec,
            keys: None,
        }
    }
}
impl Decoder for Spake2Encrypted {
    type Item = BytesMut;
    type Error = io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let mut frame = match self.length_codec.decode(src)? {
            Some(f) => f,
            None => return Ok(None),
        };
        if let Some(keys) = &self.keys {
            keys.open_in_place(&mut frame)
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "decryption failed"))?;
        }
        Ok(Some(frame))
    }
}

impl Encoder<Bytes> for Spake2Encrypted {
    type Error = io::Error;

    fn encode(&mut self, item: Bytes, dst: &mut BytesMut) -> Result<(), Self::Error> {
        if let Some(keys) = &self.keys {
            let mut buf = BytesMut::from(item);
            keys.seal_in_place(&mut buf)
                .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "encryption failed"))?;
            self.length_codec.encode(buf.freeze(), dst)
        } else {
            self.length_codec.encode(item, dst)
        }
    }
}
impl Clone for Spake2Encrypted {
    fn clone(&self) -> Self {
        Self{
            server_provider: self.server_provider.clone(),
            client_provider: self.client_provider.clone(),
            is_server: self.is_server.clone(),
            server_id: self.server_id.clone(),
            length_codec: self.length_codec.clone(),
            keys: None
        }
    }
}

#[async_trait]
impl TfCodec for Spake2Encrypted {
    async fn initial_setup(&mut self, tr: &mut Transport) -> bool {
        ///Safe limitation to prevent dos
        let length_codec = LengthDelimitedCodec::builder().max_frame_length(2048).new_codec();
        let mut framed = Framed::new(TempTransport::new(tr), length_codec);
        if self.is_server{
            let res = server_handshake(&mut framed, self.server_provider.as_ref().unwrap().clone(), self.server_id.as_slice()).await;
            if let Some(keys) = res {
                self.keys = Some(keys);
                return true;
            } else {
                return false;
            }
        } else {
            let res = client_handshake(&mut framed, self.client_provider.as_ref().unwrap().clone(), self.server_id.as_slice()).await;
            if let Some(keys) = res {
                self.keys = Some(keys);
                return true;
            }
            return false;
        }
    }
}


#[async_trait]
pub trait ServerCredentialProvider: Send+Sync+'static  {
    async fn get_client_password(&self, client_identity: &str) -> Option<Vec<u8>>;
}

#[async_trait]
pub trait ClientCredentialProvider: Send+Sync+'static {
    ///Return 0 - client identity, 1 - client password
    async fn get_client_credentials(&self) -> Option<(Vec<u8>, Vec<u8>)>;
}

pub struct SessionKeys {
    pub send: Aes256Gcm,
    pub recv: Aes256Gcm,
}


struct BytesMutBuffer(pub BytesMut);

impl AsRef<[u8]> for BytesMutBuffer {
    fn as_ref(&self) -> &[u8] { &self.0 }
}

impl AsMut<[u8]> for BytesMutBuffer {
    fn as_mut(&mut self) -> &mut [u8] { &mut self.0 }
}

impl aead::Buffer for BytesMutBuffer {
    fn extend_from_slice(&mut self, other: &[u8]) -> aead::Result<()> {
        self.0.extend_from_slice(other);
        Ok(())
    }

    fn truncate(&mut self, len: usize) {
        self.0.truncate(len);
    }
}

impl SessionKeys{
    fn derive_session_keys(shared: &[u8], is_server: bool) -> Option<Self> {
        let hk = Hkdf::<Sha256>::new(None, shared);

        let mut key_a = [0u8; 32];
        let mut key_b = [0u8; 32];

        hk.expand(b"aes-tunnel-key-a", &mut key_a).ok()?;
        hk.expand(b"aes-tunnel-key-b", &mut key_b).ok()?;

        let (send_key, recv_key) = if is_server {
            (key_b, key_a) // server sends on B, receives on A
        } else {
            (key_a, key_b) // client sends on A, receives on B
        };

        Some(Self {
            send: Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&send_key)),
            recv: Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&recv_key)),
        })
    }


    pub fn seal_in_place(&self, buf: &mut BytesMut) -> Option<()> {
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let mut wrapped = BytesMutBuffer(buf.split());

        self.send.encrypt_in_place(&nonce, b"", &mut wrapped).ok()?;

        buf.clear();
        buf.reserve(12 + wrapped.0.len());
        buf.extend_from_slice(&nonce);
        buf.unsplit(wrapped.0);
        Some(())
    }

    pub fn open_in_place(&self, buf: &mut BytesMut) -> Option<()> {
        const NONCE_LEN: usize = 12;
        if buf.len() < NONCE_LEN {
            return None;
        }
        let nonce: [u8; 12] = buf[..NONCE_LEN].try_into().ok()?;
        let ciphertext = buf.split_off(NONCE_LEN);
        let mut wrapped = BytesMutBuffer(ciphertext);

        self.recv.decrypt_in_place(&Nonce::from(nonce), b"", &mut wrapped).ok()?;

        *buf = wrapped.0;
        Some(())
    }
}

pub async fn client_handshake<'a, IO: AsyncReadWrite>(
    io: &mut Framed<TempTransport<'a, IO>, LengthDelimitedCodec>,
    cred: Arc<dyn ClientCredentialProvider>,
    server_id: &[u8],
) -> Option<SessionKeys> {
    let creds = cred.get_client_credentials().await?;
    let (spake, outbound_msg) = Spake2::<Ed25519Group>::start_a(
        &Password::new(creds.1.as_slice()),
        &Identity::new(creds.0.as_slice()),
        &Identity::new(server_id),
    );
    io.send(Bytes::from(creds.0.clone())).await.ok()?;
    io.send(Bytes::from(outbound_msg)).await.ok()?;

    let peer_msg = io.next().await?.ok()?;

    let shared = spake.finish(&peer_msg).ok()?;

    SessionKeys::derive_session_keys(&shared, false)
}

pub async fn server_handshake<'a, IO: AsyncReadWrite>(
    io: &mut Framed<TempTransport<'a, IO>, LengthDelimitedCodec>,
    cred_provider: Arc<dyn ServerCredentialProvider>,
    server_id: &[u8],
) -> Option<SessionKeys>
where
    IO: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let client_identity = io.next().await?.ok()?;
    let client_identity = String::from_utf8_lossy(client_identity.as_ref());
    let password = cred_provider.get_client_password(&client_identity).await?;
    let client_identity = client_identity.as_bytes();
    let (spake, outbound_msg) = Spake2::<Ed25519Group>::start_b(
        &Password::new(password),
        &Identity::new(client_identity),
        &Identity::new(server_id),
    );
    let peer_msg = io.next().await?.ok()?;

    io.send(Bytes::from(outbound_msg)).await.ok()?;

    let shared = spake.finish(&peer_msg).ok()?;

    SessionKeys::derive_session_keys(&shared, true)
}


