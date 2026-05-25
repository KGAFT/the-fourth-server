use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use crate::structures::transport::AsyncReadWrite;

///A container, that can store reference of the stream, and be used inside other structures, that does not accepts the references.
pub struct TempTransport<'a, T: AsyncReadWrite> {
    base_transport: &'a mut T,
}

impl<'a, T: AsyncReadWrite> TempTransport<'a, T> {
    pub fn new(transport: &'a mut T) -> Self {
        TempTransport {
            base_transport: transport,
        }
    }
}

impl<'a, T: AsyncReadWrite> AsyncRead for TempTransport<'a, T> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut *self.base_transport).poll_read(cx, buf)
    }
}

impl<'a, T: AsyncReadWrite> AsyncWrite for TempTransport<'a, T> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut *self.base_transport).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut *self.base_transport).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut *self.base_transport).poll_shutdown(cx)
    }
}
