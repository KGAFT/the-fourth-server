use std::io;
use crate::codec::codec_trait::TfCodec;
use crate::structures::temp_transport::TempTransport;
use crate::structures::transport::{AsyncReadWrite, Transport};
use aes_gcm::{
    Aes256Gcm, Key, Nonce,
    aead::{Aead, AeadCore, KeyInit},
};
use async_trait::async_trait;
use bytes::{Buf, Bytes, BytesMut};
use futures_util::{SinkExt, StreamExt};
use hkdf::Hkdf;
use sha2::Sha256;
use spake2::{Ed25519Group, Identity, Password, Spake2};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
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
        } else {
            return Err(io::Error::new(io::ErrorKind::Other, "decryption failed"));
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
            return Err(io::Error::new(io::ErrorKind::Other, "encryption failed"));
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
        //Safe limitation to prevent dos
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

    /// Local outbound packet counter
    send_counter: AtomicU64,

    /// Highest accepted inbound counter
    recv_counter: AtomicU64,
}


const COUNTER_LEN: usize = 8;
const TAG_LEN: usize = 16;


struct OffsetBuffer<'a> {
    buf: &'a mut BytesMut,
    offset: usize,
}

impl AsRef<[u8]> for OffsetBuffer<'_> {
    fn as_ref(&self) -> &[u8] { &self.buf[self.offset..] }
}

impl AsMut<[u8]> for OffsetBuffer<'_> {
    fn as_mut(&mut self) -> &mut [u8] { &mut self.buf[self.offset..] }
}

impl aead::Buffer for OffsetBuffer<'_> {
    fn extend_from_slice(&mut self, other: &[u8]) -> aead::Result<()> {
        self.buf.extend_from_slice(other);
        Ok(())
    }

    fn truncate(&mut self, len: usize) {
        self.buf.truncate(self.offset + len);
    }
}
impl SessionKeys {

        fn derive_session_keys(shared: &[u8], is_server: bool) -> Option<Self> {
            let hk = Hkdf::<Sha256>::new(None, shared);

            let mut key_a = [0u8; 32];
            let mut key_b = [0u8; 32];

            hk.expand(b"aes-tunnel-key-a", &mut key_a).ok()?;
            hk.expand(b"aes-tunnel-key-b", &mut key_b).ok()?;

            let (send_key, recv_key) = if is_server {
                (key_b, key_a)
            } else {
                (key_a, key_b)
            };

            Some(Self {
                send: Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&send_key)),
                recv: Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&recv_key)),
                send_counter: AtomicU64::new(1),
                recv_counter: AtomicU64::new(0),
            })
        }

        #[inline]
        fn nonce_from_counter(counter: u64) -> [u8; 12] {
            let mut nonce = [0u8; 12];
            nonce[4..].copy_from_slice(&counter.to_be_bytes());
            nonce
        }

        pub fn seal_in_place(&self, buf: &mut BytesMut) -> Option<()> {
            let counter = self.send_counter.fetch_add(1, Ordering::Relaxed);

            if counter == u64::MAX {
                return None;
            }

            let counter_bytes = counter.to_be_bytes();
            let nonce_bytes = Self::nonce_from_counter(counter);
            let nonce = Nonce::from_slice(&nonce_bytes);

            // Reframe the plaintext as [counter | plaintext] in a buffer sized for
            // the tag too, so neither the prefix nor the tag append reallocates.
            // The plaintext moves exactly once (the `unsplit`); after `split`, `buf`
            // is empty with no spare capacity, so the `reserve` never copies.
            let plaintext = buf.split();
            buf.reserve(COUNTER_LEN + plaintext.len() + TAG_LEN);
            buf.extend_from_slice(&counter_bytes);
            buf.unsplit(plaintext);

            // Encrypt only the bytes after the counter prefix in place; the tag
            // lands in the reserved headroom. counter is included in AAD so any
            // wire tampering fails tag verification.
            let mut framed = OffsetBuffer { buf: &mut *buf, offset: COUNTER_LEN };
            self.send
                .encrypt_in_place(nonce, &counter_bytes, &mut framed)
                .ok()?;

            Some(())
        }

        pub fn open_in_place(&self, buf: &mut BytesMut) -> Option<()> {
            if buf.len() < COUNTER_LEN {
                return None;
            }

            let counter = u64::from_be_bytes(buf[..COUNTER_LEN].try_into().ok()?);

            if counter == u64::MAX {
                return None;
            }

            // compare-exchange loop — prevents TOCTOU race if called concurrently
            let mut last = self.recv_counter.load(Ordering::Acquire);
            loop {
                if counter <= last {
                    return None; // replay or reorder
                }
                match self.recv_counter.compare_exchange_weak(
                    last,
                    counter,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                ) {
                    Ok(_) => break,
                    Err(current) => last = current, // another thread advanced it, retry
                }
            }

            let counter_bytes = counter.to_be_bytes();
            let nonce_bytes = Self::nonce_from_counter(counter);
            let nonce = Nonce::from_slice(&nonce_bytes);

            // Decrypt the ciphertext after the counter prefix in place; the tag is
            // truncated in place. AAD must match what seal used, otherwise tag fails.
            let mut framed = OffsetBuffer { buf: &mut *buf, offset: COUNTER_LEN };
            self.recv
                .decrypt_in_place(nonce, &counter_bytes, &mut framed)
                .ok()?;

            // Drop the counter prefix with a zero-copy advance so `buf` holds
            // exactly the recovered plaintext.
            buf.advance(COUNTER_LEN);

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



#[cfg(test)]
mod seal_open_tests {
    use super::*;


    fn pair() -> (SessionKeys, SessionKeys) {
        (
            SessionKeys::derive_session_keys(b"shared-secret", true).unwrap(),
            SessionKeys::derive_session_keys(b"shared-secret", false).unwrap(),
        )
    }

    fn roundtrip(plaintext: &[u8]) {
        let (server, client) = pair();

        let mut buf = BytesMut::from(plaintext);
        server.seal_in_place(&mut buf).expect("seal");
        // wire layout = counter(8) | ciphertext | tag(16)
        assert_eq!(buf.len(), COUNTER_LEN + plaintext.len() + TAG_LEN);
        client.open_in_place(&mut buf).expect("open");
        assert_eq!(&buf[..], plaintext);
    }

    #[test]
    fn roundtrip_various_sizes() {
        roundtrip(b"");
        roundtrip(b"hello");
        roundtrip(&[0x41u8; 4096]);
    }

    #[test]
    fn replay_is_rejected() {
        let (server, client) = pair();
        let mut buf = BytesMut::from(&b"first"[..]);
        server.seal_in_place(&mut buf).unwrap();
        let mut replay = buf.clone();
        client.open_in_place(&mut buf).unwrap();
        assert!(client.open_in_place(&mut replay).is_none(), "replay must be rejected");
    }

    #[test]
    fn tamper_is_rejected() {
        let (server, client) = pair();
        let mut buf = BytesMut::from(&b"payload"[..]);
        server.seal_in_place(&mut buf).unwrap();
        let idx = buf.len() - 1;
        buf[idx] ^= 0xFF; // flip a tag byte
        assert!(client.open_in_place(&mut buf).is_none(), "tamper must fail tag check");
    }
}
