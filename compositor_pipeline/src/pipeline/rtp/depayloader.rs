use std::mem;

use bytes::Bytes;
use compositor_render::error::ErrorStack;
use tracing::{info, trace, warn};
use webrtc::rtp::codecs::{h264::H264Packet, opus::OpusPacket, vp8::Vp8Packet, vp9::Vp9Packet};

use crate::prelude::*;
use crate::{
    codecs::{AacAudioSpecificConfig, AudioCodec, VideoCodec},
    pipeline::rtp::RtpPacket,
    protocols::{AacDepayloadingError, RtpAacDepayloaderMode},
};

pub use aac_depayloader::AacDepayloader;

mod aac_depayloader;

#[derive(Debug)]
pub enum DepayloaderOptions {
    H264,
    Vp8,
    Vp9,
    Opus,
    Aac(RtpAacDepayloaderMode, AacAudioSpecificConfig),
}

pub fn new_depayloader(options: DepayloaderOptions) -> Box<dyn Depayloader> {
    info!(?options, "Initialize RTP depayloader");
    match options {
        DepayloaderOptions::H264 => {
            BufferedDepayloader::<H264Packet>::new_boxed(MediaKind::Video(VideoCodec::H264))
        }
        DepayloaderOptions::Vp8 => {
            BufferedDepayloader::<Vp8Packet>::new_boxed(MediaKind::Video(VideoCodec::Vp8))
        }
        DepayloaderOptions::Vp9 => {
            BufferedDepayloader::<Vp9Packet>::new_boxed(MediaKind::Video(VideoCodec::Vp9))
        }
        DepayloaderOptions::Opus => {
            SimpleDepayloader::<OpusPacket>::new_boxed(MediaKind::Audio(AudioCodec::Opus))
        }
        DepayloaderOptions::Aac(mode, asc) => Box::new(AacDepayloader::new(mode, asc)),
    }
}

pub(crate) trait Depayloader {
    fn depayload(&mut self, packet: RtpPacket)
        -> Result<Vec<EncodedInputChunk>, DepayloadingError>;
}

#[derive(Debug, thiserror::Error)]
pub enum DepayloadingError {
    #[error(transparent)]
    Rtp(#[from] webrtc::rtp::Error),
    #[error("AAC depayloading error")]
    Aac(#[from] AacDepayloadingError),
}

struct BufferedDepayloader<T: webrtc::rtp::packetizer::Depacketizer + Default + 'static> {
    kind: MediaKind,
    buffer: Vec<Bytes>,
    depayloader: T,
}

impl<T: webrtc::rtp::packetizer::Depacketizer + Default + 'static> BufferedDepayloader<T> {
    fn new_boxed(kind: MediaKind) -> Box<dyn Depayloader> {
        Box::new(Self {
            kind,
            buffer: Vec::new(),
            depayloader: T::default(),
        })
    }
}

impl<T: webrtc::rtp::packetizer::Depacketizer + Default + 'static> Depayloader
    for BufferedDepayloader<T>
{
    fn depayload(
        &mut self,
        packet: RtpPacket,
    ) -> Result<Vec<EncodedInputChunk>, DepayloadingError> {
        trace!(?packet, "RTP depayloader received new packet");
        let chunk = self.depayloader.depacketize(&packet.packet.payload)?;

        if chunk.is_empty() {
            return Ok(Vec::new());
        }

        self.buffer.push(chunk);
        if !packet.packet.header.marker {
            // the marker bit is set on the last packet of an access unit
            return Ok(Vec::new());
        }

        let new_chunk = EncodedInputChunk {
            data: mem::take(&mut self.buffer).concat().into(),
            pts: packet.timestamp,
            dts: None,
            kind: self.kind,
        };

        trace!(chunk=?new_chunk, "RTP depayloader produced a new chunk");
        Ok(vec![new_chunk])
    }
}

struct SimpleDepayloader<T: webrtc::rtp::packetizer::Depacketizer + Default + 'static> {
    kind: MediaKind,
    depayloader: T,
}

impl<T: webrtc::rtp::packetizer::Depacketizer + Default + 'static> SimpleDepayloader<T> {
    fn new_boxed(kind: MediaKind) -> Box<dyn Depayloader> {
        Box::new(Self {
            kind,
            depayloader: T::default(),
        })
    }
}

impl<T: webrtc::rtp::packetizer::Depacketizer + Default + 'static> Depayloader
    for SimpleDepayloader<T>
{
    fn depayload(
        &mut self,
        packet: RtpPacket,
    ) -> Result<Vec<EncodedInputChunk>, DepayloadingError> {
        trace!(?packet, "RTP depayloader received new packet");
        let data = self.depayloader.depacketize(&packet.packet.payload)?;
        let chunk = EncodedInputChunk {
            data,
            pts: packet.timestamp,
            dts: None,
            kind: self.kind,
        };

        trace!(?chunk, "RTP depayloader produced a new chunk");
        Ok(vec![chunk])
    }
}

pub(crate) struct DepayloaderStream<Source>
where
    Source: Iterator<Item = PipelineEvent<RtpPacket>>,
{
    depayloader: Box<dyn Depayloader>,
    source: Source,
    eos_sent: bool,
}

impl<Source> DepayloaderStream<Source>
where
    Source: Iterator<Item = PipelineEvent<RtpPacket>>,
{
    pub fn new(options: DepayloaderOptions, source: Source) -> Self {
        Self {
            depayloader: new_depayloader(options),
            source,
            eos_sent: false,
        }
    }
}

impl<Source> Iterator for DepayloaderStream<Source>
where
    Source: Iterator<Item = PipelineEvent<RtpPacket>>,
{
    type Item = Vec<PipelineEvent<EncodedInputChunk>>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.source.next() {
            Some(PipelineEvent::Data(packet)) => match self.depayloader.depayload(packet) {
                Ok(chunks) => Some(chunks.into_iter().map(PipelineEvent::Data).collect()),
                // TODO: Remove after updating webrc-rs
                Err(DepayloadingError::Rtp(webrtc::rtp::Error::ErrShortPacket)) => Some(vec![]),
                Err(err) => {
                    warn!("Depayloader error: {}", ErrorStack::new(&err).into_string());
                    Some(vec![])
                }
            },
            Some(PipelineEvent::EOS) | None => match self.eos_sent {
                true => None,
                false => {
                    self.eos_sent = true;
                    Some(vec![PipelineEvent::EOS])
                }
            },
        }
    }
}
