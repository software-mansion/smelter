use std::mem;

use bytes::Bytes;
use tracing::{info, trace};
use webrtc::rtp::{
    self,
    codecs::{h264::H264Packet, opus::OpusPacket, vp8::Vp8Packet, vp9::Vp9Packet},
    packetizer::Depacketizer,
};

use crate::prelude::*;
use crate::{
    codecs::{AacAudioSpecificConfig, AudioCodec, VideoCodec},
    pipeline::rtp::RtpPacket,
    protocols::{AacDepayloadingError, RtpAacDepayloaderMode},
};

pub(crate) use crate::pipeline::rtp::depayloader::dynamic_stream::{
    DynamicDepayloaderStream, VideoPayloadTypeMapping,
};
pub(crate) use crate::pipeline::rtp::depayloader::static_stream::DepayloaderStream;

pub use aac_depayloader::AacDepayloader;

mod aac_depayloader;
mod dynamic_stream;
mod static_stream;

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
    Rtp(#[from] rtp::Error),
    #[error("AAC depayloading error")]
    Aac(#[from] AacDepayloadingError),
}

struct BufferedDepayloader<T: Depacketizer + Default + 'static> {
    kind: MediaKind,
    buffer: Vec<Bytes>,
    depayloader: T,
}

impl<T: Depacketizer + Default + 'static> BufferedDepayloader<T> {
    fn new_boxed(kind: MediaKind) -> Box<dyn Depayloader> {
        Box::new(Self {
            kind,
            buffer: Vec::new(),
            depayloader: T::default(),
        })
    }
}

impl<T: Depacketizer + Default + 'static> Depayloader for BufferedDepayloader<T> {
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

struct SimpleDepayloader<T: Depacketizer + Default + 'static> {
    kind: MediaKind,
    depayloader: T,
}

impl<T: Depacketizer + Default + 'static> SimpleDepayloader<T> {
    fn new_boxed(kind: MediaKind) -> Box<dyn Depayloader> {
        Box::new(Self {
            kind,
            depayloader: T::default(),
        })
    }
}

impl<T: Depacketizer + Default + 'static> Depayloader for SimpleDepayloader<T> {
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
