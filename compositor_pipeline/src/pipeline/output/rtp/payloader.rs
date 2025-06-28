use std::fmt::Debug;
use tracing::error;

use rand::Rng;
use rtp::codecs::{h264::H264Payloader, opus::OpusPayloader, vp8::Vp8Payloader, vp9::Vp9Payloader};

use crate::{
    pipeline::{types::EncodedChunk, AudioCodec, VideoCodec},
    queue::PipelineEvent,
};

pub enum PayloadedCodec {
    H264,
    Vp8,
    Vp9,
    Opus,
}

pub struct PayloaderOptions {
    pub codec: PayloadedCodec,
    pub payload_type: u8,
    pub clock_rate: u32,
    pub mtu: usize,
    pub ssrc: u32,
}

pub(crate) struct Payloader {
    payloader: Box<dyn rtp::packetizer::Payloader>,
    mtu: usize,
    ssrc: u32,
    payload_type: u8,
    clock_rate: u32,
    next_sequence_number: u16,
}

impl Payloader {
    fn new(options: PayloaderOptions) -> Self {
        let payloader: Box<dyn rtp::packetizer::Payloader> = match options.codec {
            PayloadedCodec::H264 => Box::new(H264Payloader::default()),
            PayloadedCodec::Vp8 => Box::new(Vp8Payloader::default()),
            PayloadedCodec::Vp9 => Box::new(Vp9Payloader::default()),
            PayloadedCodec::Opus => Box::new(OpusPayloader),
        };
        Self {
            ssrc: options.ssrc,
            mtu: options.mtu,
            payloader,
            payload_type: options.payload_type,
            clock_rate: options.clock_rate,
            next_sequence_number: rand::thread_rng().gen::<u16>(),
        }
    }

    pub fn payload(
        &mut self,
        chunk: EncodedChunk,
    ) -> Result<Vec<rtp::packet::Packet>, PayloadingError> {
        let payloads = self.payloader.payload(self.mtu, &chunk.data)?;
        let packets_amount = payloads.len();
        let timestamp = (chunk.pts.as_secs_f64() * self.clock_rate as f64).round() as u64;
        let timestamp = timestamp % u32::MAX as u64;

        payloads
            .into_iter()
            .enumerate()
            .map(|(i, payload)| {
                let header = rtp::header::Header {
                    version: 2,
                    padding: false,
                    extension: false,
                    marker: i == packets_amount - 1, // marker needs to be set on the last packet of each frame
                    payload_type: self.payload_type,
                    sequence_number: self.next_sequence_number,
                    timestamp: timestamp as u32,
                    ssrc: self.ssrc,
                    ..Default::default()
                };
                self.next_sequence_number = self.next_sequence_number.wrapping_add(1);

                Ok(rtp::packet::Packet { header, payload })
            })
            .collect()
    }
}

pub(crate) struct PayloaderStream<Source>
where
    Source: Iterator<Item = PipelineEvent<EncodedChunk>>,
{
    payloader: Payloader,
    source: Source,
    eos_sent: bool,
}

impl<Source> PayloaderStream<Source>
where
    Source: Iterator<Item = PipelineEvent<EncodedChunk>>,
{
    pub fn new(options: PayloaderOptions, source: Source) -> Self {
        Self {
            payloader: Payloader::new(options),
            source,
            eos_sent: false,
        }
    }
}

impl<Source> Iterator for PayloaderStream<Source>
where
    Source: Iterator<Item = PipelineEvent<EncodedChunk>>,
{
    type Item = Vec<Result<PipelineEvent<rtp::packet::Packet>, PayloadingError>>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.source.next() {
            Some(PipelineEvent::Data(chunk)) => match self.payloader.payload(chunk) {
                Ok(packets) => Some(
                    packets
                        .into_iter()
                        .map(|p| Ok(PipelineEvent::Data(p)))
                        .collect(),
                ),
                Err(err) => Some(vec![Err(err)]),
            },
            Some(PipelineEvent::EOS) | None => match self.eos_sent {
                true => None,
                false => {
                    self.eos_sent = true;
                    Some(vec![Ok(PipelineEvent::EOS)])
                }
            },
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PayloadingError {
    #[error("Tried to payload video with non video payloader.")]
    NoVideoPayloader,

    #[error("Tried to payload audio with non audio payloader.")]
    NoAudioPayloader,

    #[error(
        "Tried to payload video with codec {:#?} with payloader for codec {:#?}",
        chunk_codec,
        payloader_codec
    )]
    NonMatchingVideoCodecs {
        chunk_codec: VideoCodec,
        payloader_codec: VideoCodec,
    },

    #[error(
        "Tried to payload audio with codec {:#?} with payloader for codec {:#?}",
        chunk_codec,
        payloader_codec
    )]
    NonMatchingAudioCodecs {
        chunk_codec: AudioCodec,
        payloader_codec: AudioCodec,
    },

    #[error(transparent)]
    RtpLibError(#[from] rtp::Error),

    #[error(transparent)]
    MarshalError(#[from] webrtc_util::Error),

    #[error("Audio EOS already sent.")]
    AudioEOSAlreadySent,

    #[error("Video EOS already sent.")]
    VideoEOSAlreadySent,
}
