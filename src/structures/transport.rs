use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};
use async_tungstenite::{ByteReader, ByteWriter};
use futures_util::{StreamExt};
use pin_project::pin_project;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

#[cfg(not(target_arch = "wasm32"))]
use tokio::net::TcpStream;
#[cfg(not(target_arch = "wasm32"))]
use tokio_rustls::{client::TlsStream as ClientTlsStream, server::TlsStream as ServerTlsStream};
use tokio_tungstenite::{accept_async, connect_async};


pub struct Transport {
    inner: Box<dyn AsyncReadWrite>,
}

pub trait AsyncReadWrite: AsyncRead + AsyncWrite + Unpin + 'static {
    #[cfg(not(target_arch = "wasm32"))]
    fn is_send_sync(&self) where Self: Send + Sync {}
}

impl<T: AsyncRead + AsyncWrite + Send + Sync + Unpin + 'static> AsyncReadWrite for T {}



#[pin_project]
pub struct WsStreamCompat<R: futures_io::AsyncRead + Unpin, W: futures_io::AsyncWrite + Unpin> {
    #[pin]
    reader: R,
    #[pin]
    writer: W,
}

impl<R: futures_io::AsyncRead + Unpin, W: futures_io::AsyncWrite + Unpin> AsyncRead
for WsStreamCompat<R, W>
{
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let unfilled = buf.initialize_unfilled();
        match self.project().reader.poll_read(cx, unfilled) {
            Poll::Ready(Ok(n)) => {
                buf.advance(n);
                Poll::Ready(Ok(()))
            }
            Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl<R: futures_io::AsyncRead + Unpin, W: futures_io::AsyncWrite + Unpin> AsyncWrite
for WsStreamCompat<R, W>
{
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.project().writer.poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.project().writer.poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.project().writer.poll_close(cx)
    }
}

impl Transport {
    #[cfg(not(target_arch = "wasm32"))]
    pub fn plain(stream: TcpStream) -> Self {
        Self { inner: Box::new(stream) }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn tls_server(stream: ServerTlsStream<TcpStream>) -> Self {
        Self { inner: Box::new(stream) }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn tls_client(stream: ClientTlsStream<TcpStream>) -> Self {
        Self { inner: Box::new(stream) }
    }

    /// On WASM: connect via WebSocket, returns a Transport backed by ws_stream_wasm.
    /// On native: not available — use plain/tls_client/tls_server + a WS proxy if needed.
    #[cfg(target_arch = "wasm32")]
    pub async fn connect(url: &str) -> io::Result<Self> {
        let (_meta, ws_stream) = WsMeta::connect(url, None)
            .await
            .map_err(|e| io::Error::new(io::ErrorKind::ConnectionRefused, e.to_string()))?;

        let (reader, writer) = ws_stream.into_io().split();
        Ok(Self {
            inner: Box::new(WsStreamCompat { reader, writer }),
        })
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub async fn connect(url: &str) -> io::Result<Self> {
        let (ws_stream, _response) = connect_async(url)
            .await
            .map_err(|e| io::Error::new(io::ErrorKind::ConnectionAborted, e.to_string()))?;

        let (write, read) = ws_stream.split();
        let reader = ByteReader::new(read);
        let writer = ByteWriter::new(write);

        Ok(Self {
            inner: Box::new(WsStreamCompat { reader, writer }),
        })
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub async fn accept_websocket(stream: Transport) -> io::Result<Self> {
        let ws_stream = accept_async(stream)
            .await
            .map_err(|e| io::Error::new(io::ErrorKind::ConnectionRefused, e.to_string()))?;

        let (write, read) = ws_stream.split();
        let reader = ByteReader::new(read);
        let writer = ByteWriter::new(write);

        Ok(Self {
            inner: Box::new(WsStreamCompat { reader, writer }),
        })
    }

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
unsafe impl Send for Transport {

}
unsafe impl Sync for Transport {}