use std::sync::Arc;

use crate::pipeline::{
    decoder::{
        EncodedInputEvent, KeyframeRequestSender, VideoDecoder, VideoDecoderInstance,
        ffmpeg_utils::{create_av_packet, from_av_frame},
    },
    utils::H264AuSplitter,
};
use crate::prelude::*;

use ffmpeg_next::{
    Rational,
    codec::{Context, Id},
    media::Type,
};
use tracing::{debug, error, info, trace, warn};

const TIME_BASE: i32 = 1_000_000;

pub struct FfmpegH264Decoder {
    decoder: ffmpeg_next::decoder::Opened,
    keyframe_request_sender: Option<KeyframeRequestSender>,
    av_frame: ffmpeg_next::frame::Video,
    au_splitter: H264AuSplitter,
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
            (*decoder.as_mut_ptr()).pkt_timebase = Rational::new(1, TIME_BASE).into();
        }

        let decoder = decoder.decoder();
        let decoder = decoder.open_as(Id::H264)?;
        Ok(Self {
            decoder,
            keyframe_request_sender,
            av_frame: ffmpeg_next::frame::Video::empty(),
            au_splitter: H264AuSplitter::default(),
        })
    }
}

impl VideoDecoderInstance for FfmpegH264Decoder {
    fn decode(&mut self, event: EncodedInputEvent) -> Vec<Frame> {
        trace!(?event, "FFmpeg H264 decoder received an event.");
        let au_chunks = match event {
            EncodedInputEvent::Chunk(chunk) => match self.au_splitter.put_chunk(chunk) {
                Ok(chunks) => chunks,
                Err(err) => {
                    if let Some(s) = self.keyframe_request_sender.as_ref() {
                        s.send()
                    }
                    debug!("H264 AU splitter could not process the chunks: {err}");
                    return Vec::new();
                }
            },
            EncodedInputEvent::LostData => {
                self.au_splitter.mark_missing_data();
                return vec![];
            }
            EncodedInputEvent::Discontinuity => return self.flush(),
            EncodedInputEvent::AuDelimiter => match self.au_splitter.flush() {
                Ok(chunks) => chunks,
                Err(err) => {
                    if let Some(s) = self.keyframe_request_sender.as_ref() {
                        s.send()
                    }
                    debug!("H264 AU splitter could not process the chunks: {err}");
                    return Vec::new();
                }
            },
        };

        self.send_chunks(au_chunks);
        self.read_all_frames()
    }

    fn flush(&mut self) -> Vec<Frame> {
        // The H264 parser inside the AU splitter holds back the last access
        // unit — an AU is only emitted once the first slice of the next AU
        // arrives. Flush it so the final frame in decode order is actually
        // sent to the decoder before we drain it. Skipping this drops the
        // last AU in decode order (often a trailing B-frame), which on a
        // reordered tail leaves a doubled gap between the last two frames.
        match self.au_splitter.flush() {
            Ok(au_chunks) => self.send_chunks(au_chunks),
            Err(err) => debug!("H264 AU splitter could not be flushed: {err}"),
        }

        // Signal end of stream so the decoder drains the frames it holds back for reordering.
        let _ = self.decoder.send_eof();
        let frames = self.read_all_frames();
        // Reset decoder state, so it can accept a new stream (e.g. after discontinuity).
        self.decoder.flush();
        frames
    }
}

impl FfmpegH264Decoder {
    fn send_chunks(&mut self, au_chunks: Vec<EncodedInputChunk>) {
        for chunk in au_chunks {
            trace!(?chunk, "FFmpeg H264 processing AU chunk");
            let av_packet = match create_av_packet(chunk, VideoCodec::H264, TIME_BASE) {
                Ok(packet) => packet,
                Err(err) => {
                    warn!("Dropping frame: {}", err);
                    continue;
                }
            };

            if let Err(e) = self.decoder.send_packet(&av_packet) {
                warn!("Failed to send a packet to decoder: {:?}", e);
            }
        }
    }

    fn read_all_frames(&mut self) -> Vec<Frame> {
        let mut frames = Vec::new();
        loop {
            match self.decoder.receive_frame(&mut self.av_frame) {
                Ok(_) => match from_av_frame(&mut self.av_frame, TIME_BASE) {
                    Ok(frame) => {
                        trace!(pts=?frame.pts, "H264 decoder produced a frame.");
                        frames.push(frame);
                    }
                    Err(err) => {
                        warn!("Dropping frame: {}", err);
                        break;
                    }
                },
                Err(ffmpeg_next::Error::Eof) => break,
                Err(ffmpeg_next::Error::Other {
                    errno: ffmpeg_next::error::EAGAIN,
                }) => break, // decoder needs more chunks to produce frame
                Err(e) => {
                    error!("Decoder error: {e}.");
                    break;
                }
            }
        }
        frames
    }
}
