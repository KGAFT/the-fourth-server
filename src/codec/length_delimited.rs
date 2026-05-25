use crate::codec::codec_trait::TfCodec;
use crate::structures::transport::Transport;
use async_trait::async_trait;
use std::io;
use tokio_util::bytes::{Bytes, BytesMut};
use tokio_util::codec::{Decoder, Encoder};
#[derive(Clone, Default)]
///The wrapper aroung existing codec, to be compatible with server
pub struct LengthDelimitedCodec {
    codec: tokio_util::codec::LengthDelimitedCodec,
}

impl LengthDelimitedCodec {
    pub fn new(max_message_length: usize) -> Self {
        Self {
            codec: tokio_util::codec::LengthDelimitedCodec::builder()
                .max_frame_length(max_message_length)
                .new_codec(),
        }
    }
    
    pub fn from(codec: tokio_util::codec::LengthDelimitedCodec) -> Self {
        Self{
            codec,
        }
    }
}

impl Decoder for LengthDelimitedCodec {
    type Item = BytesMut;
    type Error = io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        self.codec.decode(src)
    }
}

impl Encoder<Bytes> for LengthDelimitedCodec {
    type Error = io::Error;
    fn encode(&mut self, item: Bytes, dst: &mut BytesMut) -> Result<(), Self::Error> {
        self.codec.encode(item, dst)
    }
}
#[async_trait]
impl TfCodec for LengthDelimitedCodec {
    async fn initial_setup(&mut self, _: &mut Transport) -> bool {
        true
    }
}
