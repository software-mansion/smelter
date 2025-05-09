use bytes::Bytes;
use std::{collections::VecDeque, fmt::Debug};
use tracing::error;
use webrtc::rtp_transceiver::PayloadType;
use webrtc_util::Marshal;

use rand::Rng;
use rtp::codecs::{h264::H264Payloader, opus::OpusPayloader, vp8::Vp8Payloader};

use crate::pipeline::{
    encoder::{AudioEncoderOptions, VideoEncoderOptions},
    types::{EncodedChunk, EncodedChunkKind},
    AudioCodec, VideoCodec,
};

const H264_CLOCK_RATE: u32 = 90000;
const VP8_CLOCK_RATE: u32 = 90000;
const OPUS_CLOCK_RATE: u32 = 48000;

struct RtpStreamContext {
    ssrc: u32,
    next_sequence_number: u16,
    received_eos: bool,
}

impl RtpStreamContext {
    pub fn new() -> Self {
        let mut rng = rand::thread_rng();
        let ssrc = rng.gen::<u32>();
        let next_sequence_number = rng.gen::<u16>();

        RtpStreamContext {
            ssrc,
            next_sequence_number,
            received_eos: false,
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

    #[error("Unsupported payload type.")]
    UnsupportedPayloadType,
}

pub struct Payloader {
    video: Option<VideoPayloader>,
    audio: Option<AudioPayloader>,
}

enum VideoPayloader {
    H264 {
        payloader: H264Payloader,
        context: RtpStreamContext,
        payload_type: PayloadType,
    },
    VP8 {
        payloader: Vp8Payloader,
        context: RtpStreamContext,
        payload_type: PayloadType,
    },
}

enum AudioPayloader {
    Opus {
        payloader: OpusPayloader,
        context: RtpStreamContext,
        payload_type: PayloadType,
    },
}

pub enum Payload {
    Video(Result<Bytes, PayloadingError>),
    Audio(Result<Bytes, PayloadingError>),
}

pub struct VideoPayloaderOptions {
    pub encoder_options: VideoEncoderOptions,
    pub payload_type: PayloadType,
}

pub struct AudioPayloaderOptions {
    pub encoder_options: AudioEncoderOptions,
    pub payload_type: PayloadType,
}

impl Payloader {
    pub fn new(video: Option<VideoPayloaderOptions>, audio: Option<AudioPayloaderOptions>) -> Self {
        Self {
            video: video.map(VideoPayloader::new),
            audio: audio.map(AudioPayloader::new),
        }
    }

    pub(super) fn payload(
        &mut self,
        mtu: usize,
        data: EncodedChunk,
    ) -> Result<VecDeque<Payload>, PayloadingError> {
        match data.kind {
            EncodedChunkKind::Video(chunk_codec) => {
                let Some(ref mut video_payloader) = self.video else {
                    return Err(PayloadingError::NoVideoPayloader);
                };

                if video_payloader.codec() != chunk_codec {
                    return Err(PayloadingError::NonMatchingVideoCodecs {
                        chunk_codec,
                        payloader_codec: video_payloader.codec(),
                    });
                }

                video_payloader.payload(mtu, data)
            }
            EncodedChunkKind::Audio(chunk_codec) => {
                let Some(ref mut audio_payloader) = self.audio else {
                    return Err(PayloadingError::NoAudioPayloader);
                };

                if audio_payloader.codec() != chunk_codec {
                    return Err(PayloadingError::NonMatchingAudioCodecs {
                        chunk_codec,
                        payloader_codec: audio_payloader.codec(),
                    });
                }

                audio_payloader.payload(mtu, data)
            }
        }
    }

    pub(super) fn audio_eos(&mut self) -> Result<Bytes, PayloadingError> {
        self.audio
            .as_mut()
            .map(|audio| {
                let ctx = audio.context_mut();
                if ctx.received_eos {
                    return Err(PayloadingError::AudioEOSAlreadySent);
                }
                ctx.received_eos = true;

                let packet = rtcp::goodbye::Goodbye {
                    sources: vec![ctx.ssrc],
                    reason: Bytes::from("Unregister output stream"),
                };
                packet.marshal().map_err(PayloadingError::MarshalError)
            })
            .unwrap_or(Err(PayloadingError::NoAudioPayloader))
    }

    pub(super) fn video_eos(&mut self) -> Result<Bytes, PayloadingError> {
        self.video
            .as_mut()
            .map(|video| {
                let ctx = video.context_mut();
                if ctx.received_eos {
                    return Err(PayloadingError::VideoEOSAlreadySent);
                }
                ctx.received_eos = true;

                let packet = rtcp::goodbye::Goodbye {
                    sources: vec![ctx.ssrc],
                    reason: Bytes::from("Unregister output stream"),
                };
                packet.marshal().map_err(PayloadingError::MarshalError)
            })
            .unwrap_or(Err(PayloadingError::NoVideoPayloader))
    }
}

impl VideoPayloader {
    fn new(codec: VideoPayloaderOptions) -> Self {
        match codec.encoder_options {
            VideoEncoderOptions::H264(_) => Self::H264 {
                payloader: H264Payloader::default(),
                context: RtpStreamContext::new(),
                payload_type: codec.payload_type,
            },
            VideoEncoderOptions::VP8(_) => Self::VP8 {
                payloader: Vp8Payloader::default(),
                context: RtpStreamContext::new(),
                payload_type: codec.payload_type,
            },
            VideoEncoderOptions::VP9(_) => todo!(),
        }
    }

    fn codec(&self) -> VideoCodec {
        match self {
            VideoPayloader::H264 { .. } => VideoCodec::H264,
            VideoPayloader::VP8 { .. } => VideoCodec::VP8,
        }
    }

    fn payload(
        &mut self,
        mtu: usize,
        chunk: EncodedChunk,
    ) -> Result<VecDeque<Payload>, PayloadingError> {
        let (payloader, context, payload_type, clock_rate): (
            &mut dyn rtp::packetizer::Payloader,
            &mut RtpStreamContext,
            PayloadType,
            u32,
        ) = match self {
            VideoPayloader::H264 {
                payloader,
                context,
                payload_type,
            } => (payloader, context, *payload_type, H264_CLOCK_RATE),
            VideoPayloader::VP8 {
                payloader,
                context,
                payload_type,
            } => (payloader, context, *payload_type, VP8_CLOCK_RATE),
        };

        let payloads = payloader.payload(mtu, &chunk.data)?;
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
                    payload_type,
                    sequence_number: context.next_sequence_number,
                    timestamp: (chunk.pts.as_secs_f64() * clock_rate as f64) as u32,
                    ssrc: context.ssrc,
                    ..Default::default()
                };
                context.next_sequence_number = context.next_sequence_number.wrapping_add(1);

                Ok(Payload::Video(Ok(
                    rtp::packet::Packet { header, payload }.marshal()?
                )))
            })
            .collect()
    }

    fn context_mut(&mut self) -> &mut RtpStreamContext {
        match self {
            VideoPayloader::H264 { context, .. } => context,
            VideoPayloader::VP8 { context, .. } => context,
        }
    }
}

impl AudioPayloader {
    fn new(codec: AudioPayloaderOptions) -> Self {
        match codec.encoder_options {
            AudioEncoderOptions::Opus(_) => Self::Opus {
                payloader: OpusPayloader,
                context: RtpStreamContext::new(),
                payload_type: codec.payload_type,
            },
            AudioEncoderOptions::Aac(_) => panic!("Aac audio output is not supported yet"),
        }
    }

    fn codec(&self) -> AudioCodec {
        match self {
            AudioPayloader::Opus { .. } => AudioCodec::Opus,
        }
    }

    fn payload(
        &mut self,
        mtu: usize,
        chunk: EncodedChunk,
    ) -> Result<VecDeque<Payload>, PayloadingError> {
        let (payloader, context, payload_type, clock_rate): (
            &mut dyn rtp::packetizer::Payloader,
            &mut RtpStreamContext,
            PayloadType,
            u32,
        ) = match self {
            AudioPayloader::Opus {
                payloader,
                context,
                payload_type,
            } => (payloader, context, *payload_type, OPUS_CLOCK_RATE),
        };

        let payloads = payloader.payload(mtu, &chunk.data)?;
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
                    payload_type,
                    sequence_number: context.next_sequence_number,
                    timestamp: (chunk.pts.as_secs_f64() * clock_rate as f64) as u32,
                    ssrc: context.ssrc,
                    ..Default::default()
                };
                context.next_sequence_number = context.next_sequence_number.wrapping_add(1);

                Ok(Payload::Audio(Ok(
                    rtp::packet::Packet { header, payload }.marshal()?
                )))
            })
            .collect()
    }

    fn context_mut(&mut self) -> &mut RtpStreamContext {
        match self {
            AudioPayloader::Opus { context, .. } => context,
        }
    }
}
