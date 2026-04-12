use crate::codec::codec_trait::TfCodec;
use crate::structures::transport::Transport;
use async_trait::async_trait;
use tokio_util::bytes::{BytesMut};
use tokio_util::codec::{ Framed};

#[async_trait]
///A traffic processor trait, that applied to all streams. Processes all stream. If you need setup by each specific stream, use codecs instead
pub trait TrafficProcess: Send + Sync {
    type Codec;
    ///The routine that defines if we can connect stream or not
    async fn initial_connect(&mut self, source: &mut Transport) -> bool;
    ///The routine that defines if we can connect stream or not, but when framed was setted up
    async fn initial_framed_connect(&mut self, source: &mut Framed<Transport, Self::Codec>)
    -> bool;
    ///Process every traffic that is handled by server
    async fn post_process_traffic(&mut self, data: Vec<u8>) -> Vec<u8>;
    ///Process every traffic that is handled by server
    async fn pre_process_traffic(&mut self, data: BytesMut) -> BytesMut;
    fn clone(&self) -> Box<dyn TrafficProcess<Codec = Self::Codec>>;
}

pub struct TrafficProcessorHolder<C>
where
    C: TfCodec,
{
    processors: Vec<Box<dyn TrafficProcess<Codec = C>>>,
}

impl<C> Clone for TrafficProcessorHolder<C>
where
    C: TfCodec,
{
    fn clone(&self) -> Self {
        let mut processors = Vec::new();
        self.processors
            .iter()
            .for_each(|p| processors.push(p.as_ref().clone()));

        Self { processors }
    }
}

impl<C> TrafficProcessorHolder<C>
where
    C: TfCodec,
{
    pub fn new() -> Self {
        TrafficProcessorHolder { processors: vec![] }
    }
    pub fn register_processor(&mut self, processor: Box<dyn TrafficProcess<Codec = C>>) {
        self.processors.push(processor);
    }

    pub async fn initial_connect(&mut self, source: &mut Transport) -> bool {
        for processor in self.processors.iter_mut() {
            if !processor.as_mut().initial_connect(source).await {
                return false;
            }
        }
        true
    }

    pub async fn initial_framed_connect(&mut self, source: &mut Framed<Transport, C>) -> bool {
        for processor in self.processors.iter_mut() {
            if !processor.as_mut().initial_framed_connect(source).await {
                return false;
            }
        }
        true
    }

    pub async fn post_process_traffic(&mut self, mut data: Vec<u8>) -> Vec<u8> {
        for proc in self.processors.iter_mut() {
            data = proc.post_process_traffic(data).await;
        }
        data
    }

    pub async fn pre_process_traffic(&mut self, mut data: BytesMut) -> BytesMut {
        for proc in self.processors.iter_mut() {
            data = proc.pre_process_traffic(data).await;
        }
        data
    }
}
