use std::{iter, sync::Arc};

use crate::pipeline::decoder::{
    VideoDecoder, VideoDecoderInstance,
    ffmpeg_utils::{create_av_packet, from_av_frame},
};
use crate::prelude::*;

use ffmpeg_next::{
    Rational,
    codec::{Context, Id},
    media::Type,
};
use smelter_render::Frame;
use tracing::{error, info, trace, warn};

const TIME_BASE: i32 = 1_000_000;

pub struct FfmpegVp9Decoder {
    decoder: ffmpeg_next::decoder::Opened,
    av_frame: ffmpeg_next::frame::Video,
}

impl VideoDecoder for FfmpegVp9Decoder {
    const LABEL: &'static str = "FFmpeg VP9 decoder";

    fn new(_ctx: &Arc<PipelineCtx>) -> Result<Self, DecoderInitError> {
        info!("Initializing FFmpeg VP9 decoder");
        let mut parameters = ffmpeg_next::codec::Parameters::new();
        unsafe {
            let parameters = &mut *parameters.as_mut_ptr();

            parameters.codec_type = Type::Video.into();
            parameters.codec_id = Id::VP9.into();
        };

        let mut decoder = Context::from_parameters(parameters)?;
        unsafe {
            (*decoder.as_mut_ptr()).pkt_timebase = Rational::new(1, TIME_BASE).into();
        }

        let decoder = decoder.decoder();
        let decoder = decoder.open_as(Id::VP9)?;
        Ok(Self {
            decoder,
            av_frame: ffmpeg_next::frame::Video::empty(),
        })
    }
}

impl VideoDecoderInstance for FfmpegVp9Decoder {
    fn decode(&mut self, chunk: EncodedInputChunk) -> Vec<Frame> {
        let av_packet = match create_av_packet(chunk, VideoCodec::Vp9, TIME_BASE) {
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

impl FfmpegVp9Decoder {
    fn read_all_frames(&mut self) -> Vec<Frame> {
        iter::from_fn(|| {
            match self.decoder.receive_frame(&mut self.av_frame) {
                Ok(_) => match from_av_frame(&mut self.av_frame, TIME_BASE) {
                    Ok(frame) => {
                        trace!(pts=?frame.pts, "VP9 decoder produced a frame.");
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
