use bytes::Bytes;
use std::{collections::VecDeque, fmt::Debug};
use tracing::error;
use webrtc_util::Marshal;

use rand::Rng;
use rtp::codecs::{h264::H264Payloader, opus::OpusPayloader, vp8::Vp8Payloader, vp9::Vp9Payloader};

use crate::pipeline::{
    encoder::{AudioEncoderOptions, VideoEncoderOptions},
    rtp::{AUDIO_PAYLOAD_TYPE, VIDEO_PAYLOAD_TYPE},
    types::{EncodedChunk, EncodedChunkKind},
    AudioCodec, VideoCodec,
};

const H264_CLOCK_RATE: u32 = 90000;
const VP8_CLOCK_RATE: u32 = 90000;
const VP9_CLOCK_RATE: u32 = 90000;
const OPUS_CLOCK_RATE: u32 = 48000;

//struct RtpStreamContext {
//    ssrc: u32,
//    next_sequence_number: u16,
//    received_eos: bool,
//}
//
//impl RtpStreamContext {
//    pub fn new() -> Self {
//        let mut rng = rand::thread_rng();
//        let ssrc = rng.gen::<u32>();
//        let next_sequence_number = rng.gen::<u16>();
//
//        RtpStreamContext {
//            ssrc,
//            next_sequence_number,
//            received_eos: false,
//        }
//    }
//}

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

//    pub(super) fn audio_eos(&mut self) -> Result<Bytes, PayloadingError> {
//        self.audio
//            .as_mut()
//            .map(|audio| {
//                let ctx = audio.context_mut();
//                if ctx.received_eos {
//                    return Err(PayloadingError::AudioEOSAlreadySent);
//                }
//                ctx.received_eos = true;
//
//                let packet = rtcp::goodbye::Goodbye {
//                    sources: vec![ctx.ssrc],
//                    reason: Bytes::from("Unregister output stream"),
//                };
//                packet.marshal().map_err(PayloadingError::MarshalError)
//            })
//            .unwrap_or(Err(PayloadingError::NoAudioPayloader))
//    }
//
//    pub(super) fn video_eos(&mut self) -> Result<Bytes, PayloadingError> {
//        self.video
//            .as_mut()
//            .map(|video| {
//                let ctx = video.context_mut();
//                if ctx.received_eos {
//                    return Err(PayloadingError::VideoEOSAlreadySent);
//                }
//                ctx.received_eos = true;
//
//                let packet = rtcp::goodbye::Goodbye {
//                    sources: vec![ctx.ssrc],
//                    reason: Bytes::from("Unregister output stream"),
//                };
//                packet.marshal().map_err(PayloadingError::MarshalError)
//            })
//            .unwrap_or(Err(PayloadingError::NoVideoPayloader))
//    }
//}

pub enum PayloadedCodec {
    H264,
    Vp8,
    Vp9,
    Opus,
}

pub struct PayloaderOptions {
    codec: PayloadedCodec,
    payload_type: u8,
    clock_rate: u32,
    mtu: usize,
    ssrc: u32,
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
            PayloadedCodec::Opus => Box::new(OpusPayloader::default()),
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
    ) -> Result<VecDeque<rtp::packet::Packet>, PayloadingError> {
        let payloads = self.payloader.payload(self.mtu, &chunk.data)?;
        let packets_amount = payloads.len();

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
                    timestamp: (chunk.pts.as_secs_f64() * self.clock_rate as f64) as u32,
                    ssrc: self.ssrc,
                    ..Default::default()
                };
                self.next_sequence_number = self.next_sequence_number.wrapping_add(1);

                Ok(rtp::packet::Packet { header, payload })
            })
            .collect()
    }
}
