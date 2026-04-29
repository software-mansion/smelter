use std::{iter, sync::Arc};

use crate::pipeline::{
    decoder::{
        EncodedInputEvent, KeyframeRequestSender, VideoDecoder, VideoDecoderInstance,
        ffmpeg_utils::{create_av_packet, from_av_frame},
    },
    utils::{AuSplitterError, H264AuSplitter},
};
use crate::prelude::*;

use ffmpeg_next::{
    Rational,
    codec::{Context, Id},
    ffi::AV_CODEC_FLAG2_CHUNKS,
    media::Type,
};
use smelter_render::Frame;
use tracing::{debug, error, info, trace, warn};

const TIME_BASE: i32 = 1_000_000;

pub struct FfmpegH264Decoder {
    decoder: ffmpeg_next::decoder::Opened,
    keyframe_request_sender: Option<KeyframeRequestSender>,
    av_frame: ffmpeg_next::frame::Video,
    au_splitter: H264AuSplitter,
    use_au_splitter: bool,
    drop_frames: bool,
}

impl VideoDecoder for FfmpegH264Decoder {
    const LABEL: &'static str = "FFmpeg H264 decoder";

    fn new(
        _ctx: &Arc<PipelineCtx>,
        keyframe_request_sender: Option<KeyframeRequestSender>,
    ) -> Result<Self, DecoderInitError> {
        info!("Initializing FFmpeg H264 decoder");
        let mut parameters = ffmpeg_next::codec::Parameters::new();
        unsafe {
            let parameters = &mut *parameters.as_mut_ptr();

            parameters.codec_type = Type::Video.into();
            parameters.codec_id = Id::H264.into();
        };

        let mut decoder = Context::from_parameters(parameters)?;
        unsafe {
            (*decoder.as_mut_ptr()).flags2 |= AV_CODEC_FLAG2_CHUNKS;
            (*decoder.as_mut_ptr()).pkt_timebase = Rational::new(1, TIME_BASE).into();
        }

        let decoder = decoder.decoder();
        let decoder = decoder.open_as(Id::H264)?;
        Ok(Self {
            decoder,
            keyframe_request_sender,
            av_frame: ffmpeg_next::frame::Video::empty(),
            au_splitter: H264AuSplitter::default(),
            use_au_splitter: true,
            drop_frames: false,
        })
    }
}

impl VideoDecoderInstance for FfmpegH264Decoder {
    fn decode(&mut self, event: EncodedInputEvent) -> Vec<Frame> {
        trace!(?event, "FFmpeg H264 decoder received an event.");
        let au_chunks = match event {
            EncodedInputEvent::Chunk(chunk) => {
                self.drop_frames = !chunk.present;
                if !self.use_au_splitter {
                    vec![chunk]
                } else {
                    match self.au_splitter.put_chunk(&chunk) {
                        Ok(chunks) => chunks,
                        Err(err) => {
                            if let Some(s) = self.keyframe_request_sender.as_ref() {
                                s.send()
                            }

                            if should_disable_au_splitter(&err) {
                                // If AU splitting is incompatible with this incoming packetization,
                                // keep decoding in chunk mode instead of stalling on repeated errors.
                                self.use_au_splitter = false;
                                self.au_splitter = H264AuSplitter::default();
                                debug!(
                                    "H264 AU splitter failed: {err}. Disabling AU splitter and falling back to direct chunk decode."
                                );
                                vec![chunk]
                            } else {
                                debug!(
                                    "H264 AU splitter reported transient stream issue: {err}. Keeping AU splitter enabled and waiting for keyframe recovery."
                                );
                                Vec::new()
                            }
                        }
                    }
                }
            }
            EncodedInputEvent::LostData => {
                if self.use_au_splitter {
                    self.au_splitter.mark_missing_data();
                }
                if let Some(s) = self.keyframe_request_sender.as_ref() {
                    s.send()
                }
                return vec![];
            }
            EncodedInputEvent::AuDelimiter => {
                if !self.use_au_splitter {
                    Vec::new()
                } else {
                    match self.au_splitter.flush() {
                        Ok(chunks) => chunks,
                        Err(err) => {
                            if let Some(s) = self.keyframe_request_sender.as_ref() {
                                s.send()
                            }
                            if should_disable_au_splitter(&err) {
                                self.use_au_splitter = false;
                                self.au_splitter = H264AuSplitter::default();
                                debug!(
                                    "H264 AU splitter flush failed: {err}. Disabling AU splitter and continuing in direct chunk mode."
                                );
                            } else {
                                debug!(
                                    "H264 AU splitter flush reported transient stream issue: {err}. Keeping AU splitter enabled."
                                );
                            }
                            Vec::new()
                        }
                    }
                }
            }
        };

        for chunk in au_chunks {
            trace!(?chunk, "FFmpeg H264 processing AU chunk");
            let av_packet = match create_av_packet(chunk, VideoCodec::H264, TIME_BASE) {
                Ok(packet) => packet,
                Err(err) => {
                    warn!("Dropping frame: {}", err);
                    continue;
                }
            };

            match self.decoder.send_packet(&av_packet) {
                Ok(()) => {}
                Err(e) => {
                    warn!("Failed to send a packet to decoder: {:?}", e);
                    continue;
                }
            }
        }

        self.read_all_frames()
    }

    fn flush(&mut self) -> Vec<Frame> {
        self.decoder.flush();
        self.read_all_frames()
    }
}

fn should_disable_au_splitter(err: &AuSplitterError) -> bool {
    matches!(
        err,
        AuSplitterError::ParserError(_)
            | AuSplitterError::InvalidAccessUnit
            | AuSplitterError::UnsupportedMediaKind(_)
    )
}

impl FfmpegH264Decoder {
    fn read_all_frames(&mut self) -> Vec<Frame> {
        iter::from_fn(|| {
            match self.decoder.receive_frame(&mut self.av_frame) {
                Ok(_) => match from_av_frame(&mut self.av_frame, TIME_BASE) {
                    Ok(frame) => {
                        trace!(pts=?frame.pts, drop_frames=?self.drop_frames, "H264 decoder produced a frame.");
                        match self.drop_frames {
                            true => None,
                            false => Some(frame),
                        }
                    }
                    Err(err) => {
                        warn!("Dropping frame: {}", err);
                        None
                    }
                },
                Err(ffmpeg_next::Error::Eof) => None,
                Err(ffmpeg_next::Error::Other {
                    errno: ffmpeg_next::error::EAGAIN,
                }) => None, // decoder needs more chunks to produce frame
                Err(e) => {
                    error!("Decoder error: {e}.");
                    None
                }
            }
        })
        .collect()
    }
}
