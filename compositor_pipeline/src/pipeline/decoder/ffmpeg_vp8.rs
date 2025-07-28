use std::{iter, sync::Arc};

use crate::pipeline::decoder::{
    ffmpeg_utils::{create_av_packet, from_av_frame},
    VideoDecoder, VideoDecoderInstance,
};
use crate::prelude::*;

use compositor_render::Frame;
use ffmpeg_next::{
    codec::{Context, Id},
    media::Type,
    Rational,
};
use tracing::{error, info, trace, warn};

const TIME_BASE: i32 = 1_000_000;

pub struct FfmpegVp8Decoder {
    decoder: ffmpeg_next::decoder::Opened,
    av_frame: ffmpeg_next::frame::Video,
}

impl VideoDecoder for FfmpegVp8Decoder {
    const LABEL: &'static str = "FFmpeg VP8 decoder";

    fn new(_ctx: &Arc<PipelineCtx>) -> Result<Self, DecoderInitError> {
        info!("Initializing FFmpeg VP8 decoder");
        let mut parameters = ffmpeg_next::codec::Parameters::new();
        unsafe {
            let parameters = &mut *parameters.as_mut_ptr();

            parameters.codec_type = Type::Video.into();
            parameters.codec_id = Id::VP8.into();
        };

        let mut decoder = Context::from_parameters(parameters)?;
        unsafe {
            (*decoder.as_mut_ptr()).pkt_timebase = Rational::new(1, TIME_BASE).into();
        }

        let decoder = decoder.decoder();
        let decoder = decoder.open_as(Id::VP8)?;
        Ok(Self {
            decoder,
            av_frame: ffmpeg_next::frame::Video::empty(),
        })
    }
}

impl VideoDecoderInstance for FfmpegVp8Decoder {
    fn decode(&mut self, chunk: EncodedInputChunk) -> Vec<Frame> {
        let av_packet = match create_av_packet(chunk, VideoCodec::Vp8, TIME_BASE) {
            Ok(packet) => packet,
            Err(err) => {
                warn!("Dropping frame: {}", err);
                return Vec::new();
            }
        };

        match self.decoder.send_packet(&av_packet) {
            Ok(()) => {}
            Err(e) => {
                warn!("Failed to send a packet to decoder: {:?}", e);
                return Vec::new();
            }
        }
        self.read_all_frames()
    }

    fn flush(&mut self) -> Vec<Frame> {
        self.decoder.flush();
        self.read_all_frames()
    }
}

impl FfmpegVp8Decoder {
    fn read_all_frames(&mut self) -> Vec<Frame> {
        iter::from_fn(|| {
            match self.decoder.receive_frame(&mut self.av_frame) {
                Ok(_) => match from_av_frame(&mut self.av_frame, TIME_BASE) {
                    Ok(frame) => {
                        trace!(pts=?frame.pts, "VP8 decoder produced a frame.");
                        Some(frame)
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
