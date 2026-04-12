use async_trait::async_trait;
use tokio::io;
use tokio_util::bytes::{Bytes, BytesMut};
use tokio_util::codec::{Decoder, Encoder};
use crate::structures::transport::Transport;

#[async_trait]
///The additional trait that gaves ability to setup codec per connection.
pub trait TfCodec: Encoder<Bytes, Error = io::Error>
+ Decoder<Item = BytesMut, Error = io::Error>
+ Send
+ Sync
+ Clone
+ 'static
{
    async fn initial_setup(&mut self, transport: &mut Transport) -> bool;
}
