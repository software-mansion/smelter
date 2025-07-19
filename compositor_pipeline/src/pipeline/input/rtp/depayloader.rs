use std::mem;

use bytes::Bytes;
use compositor_render::error::ErrorStack;
use rtp::codecs::{h264::H264Packet, opus::OpusPacket, vp8::Vp8Packet, vp9::Vp9Packet};
use tracing::{info, trace, warn};

use crate::{
    pipeline::{
        input::rtp::RtpAacDepayloaderMode,
        output::rtp::RtpPacket,
        types::{AudioCodec, EncodedChunk, EncodedChunkKind, IsKeyframe, VideoCodec},
    },
    queue::PipelineEvent,
};

pub use aac_asc::AudioSpecificConfig;
pub use aac_depayloader::{AacDepayloader, AacDepayloadingError};

mod aac_asc;
mod aac_depayloader;

#[derive(Debug)]
pub enum DepayloaderOptions {
    H264,
    Vp8,
    Vp9,
    Opus,
    Aac(RtpAacDepayloaderMode, AudioSpecificConfig),
}

pub fn new_depayloader(options: DepayloaderOptions) -> Box<dyn Depayloader> {
    info!(?options, "Initialize RTP depayloader");
    match options {
        DepayloaderOptions::H264 => {
            BufferedDepayloader::<H264Packet>::new_boxed(EncodedChunkKind::Video(VideoCodec::H264))
        }
        DepayloaderOptions::Vp8 => {
            BufferedDepayloader::<Vp8Packet>::new_boxed(EncodedChunkKind::Video(VideoCodec::Vp8))
        }
        DepayloaderOptions::Vp9 => {
            BufferedDepayloader::<Vp9Packet>::new_boxed(EncodedChunkKind::Video(VideoCodec::Vp9))
        }
        DepayloaderOptions::Opus => {
            SimpleDepayloader::<OpusPacket>::new_boxed(EncodedChunkKind::Audio(AudioCodec::Opus))
        }
        DepayloaderOptions::Aac(mode, asc) => Box::new(AacDepayloader::new(mode, asc)),
    }
}

pub(crate) trait Depayloader {
    fn depayload(&mut self, packet: RtpPacket) -> Result<Vec<EncodedChunk>, DepayloadingError>;
}

#[derive(Debug, thiserror::Error)]
pub enum DepayloadingError {
    #[error(transparent)]
    Rtp(#[from] rtp::Error),
    #[error("AAC depayloading error")]
    Aac(#[from] AacDepayloadingError),
}

struct BufferedDepayloader<T: rtp::packetizer::Depacketizer + Default + 'static> {
    kind: EncodedChunkKind,
    buffer: Vec<Bytes>,
    depayloader: T,
}

impl<T: rtp::packetizer::Depacketizer + Default + 'static> BufferedDepayloader<T> {
    fn new_boxed(kind: EncodedChunkKind) -> Box<dyn Depayloader> {
        Box::new(Self {
            kind,
            buffer: Vec::new(),
            depayloader: T::default(),
        })
    }
}

impl<T: rtp::packetizer::Depacketizer + Default + 'static> Depayloader for BufferedDepayloader<T> {
    fn depayload(&mut self, packet: RtpPacket) -> Result<Vec<EncodedChunk>, DepayloadingError> {
        let chunk = self.depayloader.depacketize(&packet.packet.payload)?;
        trace!(?chunk, header=?packet.packet.header, "RTP depayloader received new chunk");

        if chunk.is_empty() {
            return Ok(Vec::new());
        }

        self.buffer.push(chunk);
        if !packet.packet.header.marker {
            // the marker bit is set on the last packet of an access unit
            return Ok(Vec::new());
        }

        let new_chunk = EncodedChunk {
            data: mem::take(&mut self.buffer).concat().into(),
            pts: packet.timestamp,
            dts: None,
            is_keyframe: IsKeyframe::Unknown,
            kind: self.kind,
        };

        trace!(chunk=?new_chunk, "RTP depayloader produced new chunk");
        Ok(vec![new_chunk])
    }
}

struct SimpleDepayloader<T: rtp::packetizer::Depacketizer + Default + 'static> {
    kind: EncodedChunkKind,
    depayloader: T,
}

impl<T: rtp::packetizer::Depacketizer + Default + 'static> SimpleDepayloader<T> {
    fn new_boxed(kind: EncodedChunkKind) -> Box<dyn Depayloader> {
        Box::new(Self {
            kind,
            depayloader: T::default(),
        })
    }
}

impl<T: rtp::packetizer::Depacketizer + Default + 'static> Depayloader for SimpleDepayloader<T> {
    fn depayload(&mut self, packet: RtpPacket) -> Result<Vec<EncodedChunk>, DepayloadingError> {
        let data = self.depayloader.depacketize(&packet.packet.payload)?;
        let chunk = EncodedChunk {
            data,
            pts: packet.timestamp,
            dts: None,
            is_keyframe: IsKeyframe::Unknown,
            kind: self.kind,
        };

        trace!(?chunk, "RTP depayloader produced new chunk");
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
    type Item = Vec<PipelineEvent<EncodedChunk>>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.source.next() {
            Some(PipelineEvent::Data(packet)) => match self.depayloader.depayload(packet) {
                Ok(chunks) => Some(chunks.into_iter().map(PipelineEvent::Data).collect()),
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
