use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::{Bytes, BytesMut};
use futures_util::{Sink, Stream};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;
use tokio_rustls::{client::TlsStream as ClientTlsStream, server::TlsStream as ServerTlsStream};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
use tungstenite::Message;

/// Unified transport wrapper, for different types of streams
pub struct Transport {
    inner: Box<dyn AsyncReadWrite>,
}

/// Trait object to unify AsyncRead + AsyncWrite
pub trait AsyncReadWrite: AsyncRead + AsyncWrite + Send + Sync + Unpin + 'static {}
impl<T: AsyncRead + AsyncWrite + ?Sized + Send + Sync + Unpin + 'static> AsyncReadWrite for T {}

// ── WebSocket compatibility shim ──────────────────────────────────────────────

/// Wraps a `WebSocketStream` and exposes it as `AsyncRead + AsyncWrite` by:
///
/// - **Read side**: polls the WS `Stream` for the next message, accumulates its
///   payload bytes in `read_buf`, and drains that buffer on every `poll_read`.
/// - **Write side**: accumulates bytes written via `poll_write` into `write_buf`,
///   then sends them as a single Binary frame on `poll_flush` / `poll_shutdown`.
pub struct WebSocketCompat<S> {
    inner: WebSocketStream<S>,
    /// Leftover bytes from the last received message not yet consumed by the reader.
    read_buf: BytesMut,
    /// Bytes accumulated since the last flush, to be sent as one Binary frame.
    write_buf: BytesMut,
}

impl<S: AsyncRead + AsyncWrite + Unpin + Send + Sync + 'static> WebSocketCompat<S> {
    pub fn new(ws: WebSocketStream<S>) -> Self {
        Self {
            inner: ws,
            read_buf: BytesMut::new(),
            write_buf: BytesMut::new(),
        }
    }
}

impl<S: AsyncRead + AsyncWrite + Unpin + Send + Sync + 'static> AsyncRead for WebSocketCompat<S> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = &mut *self;

        // Keep pulling frames until we have data or the stream is exhausted.
        loop {
            // 1. Drain whatever is already buffered first.
            if !this.read_buf.is_empty() {
                let amt = std::cmp::min(buf.remaining(), this.read_buf.len());
                buf.put_slice(&this.read_buf.split_to(amt));
                return Poll::Ready(Ok(()));
            }

            // 2. Ask the WebSocket for the next message.
            match Pin::new(&mut this.inner).poll_next(cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(None) => return Poll::Ready(Ok(())), // clean close → EOF
                Poll::Ready(Some(Err(e))) => {
                    return Poll::Ready(Err(io::Error::new(io::ErrorKind::Other, e)))
                }
                Poll::Ready(Some(Ok(msg))) => {
                    let payload: Bytes = match msg {
                        // Carry the raw bytes of Binary / Text frames.
                        Message::Binary(data) => data,
                        Message::Text(text) => Bytes::from(text.as_bytes().to_vec()),
                        // Tungstenite handles Ping/Pong/Close internally;
                        // surface a close as EOF, skip everything else.
                        Message::Close(_) => return Poll::Ready(Ok(())),
                        _ => continue,
                    };
                    this.read_buf.extend_from_slice(&payload);
                    // Loop back to drain the newly filled buffer.
                }
            }
        }
    }
}

impl<S: AsyncRead + AsyncWrite + Unpin + Send + Sync + 'static> AsyncWrite for WebSocketCompat<S> {
    /// Accumulate raw bytes — we can't send a partial WebSocket frame, so we
    /// buffer everything until `poll_flush` / `poll_shutdown` is called.
    fn poll_write(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.write_buf.extend_from_slice(buf);
        Poll::Ready(Ok(buf.len()))
    }

    /// Drain `write_buf` as a single Binary frame, then flush the underlying sink.
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = &mut *self;

        // Ensure the sink is ready to accept a new item.
        if let Poll::Pending = Pin::new(&mut this.inner)
            .poll_ready(cx)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?
        {
            return Poll::Pending;
        }

        // Send accumulated bytes as one Binary frame (if any).
        if !this.write_buf.is_empty() {
            let payload = this.write_buf.split().freeze();
            Pin::new(&mut this.inner)
                .start_send(Message::binary(payload))
                .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        }

        // Flush the underlying sink.
        Pin::new(&mut this.inner)
            .poll_flush(cx)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        // Send a WebSocket Close frame and flush.
        Pin::new(&mut self.inner)
            .poll_close(cx)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))
    }
}

// ── Transport ─────────────────────────────────────────────────────────────────

impl Transport {
    /// Wrap a plain TcpStream.
    pub fn plain(stream: TcpStream) -> Self {
        Self {
            inner: Box::new(stream),
        }
    }

    /// Wrap a server-side TLS stream.
    pub fn tls_server(stream: ServerTlsStream<TcpStream>) -> Self {
        Self {
            inner: Box::new(stream),
        }
    }

    /// Wrap a client-side TLS stream.
    pub fn tls_client(stream: ClientTlsStream<TcpStream>) -> Self {
        Self {
            inner: Box::new(stream),
        }
    }

    /// Wrap a plain (non-TLS) WebSocket stream.
    pub fn websocket(stream: WebSocketStream<TcpStream>) -> Self {
        Self {
            inner: Box::new(WebSocketCompat::new(stream)),
        }
    }

    /// Wrap a WebSocket stream whose transport may itself be plain or TLS
    /// (`tokio_tungstenite::MaybeTlsStream`).
    pub fn websocket_maybe_tls(stream: WebSocketStream<MaybeTlsStream<TcpStream>>) -> Self {
        Self {
            inner: Box::new(WebSocketCompat::new(stream)),
        }
    }

    /// Optionally expose inner (if needed).
    pub fn inner(&mut self) -> &mut dyn AsyncReadWrite {
        &mut *self.inner
    }
}

impl AsyncRead for Transport {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut *self.inner).poll_read(cx, buf)
    }
}

impl AsyncWrite for Transport {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut *self.inner).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut *self.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut *self.inner).poll_shutdown(cx)
    }
}