use std::sync::Arc;

use crate::pipeline::{
    decoder::{
        EncodedInputEvent, KeyframeRequestSender, VideoDecoder, VideoDecoderInstance,
        ffmpeg_utils::{TIME_BASE, create_av_packet, from_av_frame},
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
            EncodedInputEvent::Chunk(chunk) => self.au_splitter.put_chunk(chunk),
            EncodedInputEvent::LostData => {
                self.au_splitter.mark_missing_data();
                return vec![];
            }
            EncodedInputEvent::Discontinuity => return self.flush(),
            EncodedInputEvent::AuDelimiter => self.au_splitter.flush(),
        };

        let mut frames = Vec::new();
        for chunk in au_chunks {
            match chunk {
                Ok(chunk) => {
                    self.send_chunk(chunk);
                    // Decoder rejects the next packet with EAGAIN until its output is read.
                    frames.extend(self.read_all_frames());
                }
                Err(err) => {
                    if let Some(s) = &self.keyframe_request_sender {
                        s.send()
                    }
                    debug!("H264 AU splitter could not process the chunk: {err}");
                }
            }
        }
        frames
    }

    fn flush(&mut self) -> Vec<Frame> {
        let mut frames = Vec::new();
        for chunk in self.au_splitter.flush() {
            match chunk {
                Ok(chunk) => {
                    self.send_chunk(chunk);
                    frames.extend(self.read_all_frames());
                }
                Err(err) => debug!("H264 AU splitter could not be flushed: {err}"),
            }
        }
        self.au_splitter.reset();

        // Signal end of stream so the decoder drains the frames it holds back for reordering.
        let _ = self.decoder.send_eof();
        frames.extend(self.read_all_frames());
        // Reset decoder state, so it can accept a new stream (e.g. after discontinuity).
        self.decoder.flush();
        frames
    }
}

impl FfmpegH264Decoder {
    fn send_chunk(&mut self, chunk: EncodedInputChunk) {
        trace!(?chunk, "FFmpeg H264 processing AU chunk");
        let av_packet = match create_av_packet(chunk, VideoCodec::H264) {
            Ok(packet) => packet,
            Err(err) => {
                warn!("Dropping frame: {}", err);
                return;
            }
        };

        if let Err(e) = self.decoder.send_packet(&av_packet) {
            // Decoder rejected data the splitter accepted, e.g. a picture referencing parameter
            // sets that were dropped with an incomplete access unit.
            warn!("Failed to send a packet to decoder: {:?}", e);
            // Sent in decode order, so there is no reorder delay to account for.
            self.au_splitter.wait_for_recovery_point(None);
            if let Some(s) = &self.keyframe_request_sender {
                s.send()
            }
        }
    }

    fn read_all_frames(&mut self) -> Vec<Frame> {
        let mut frames = Vec::new();
        loop {
            match self.decoder.receive_frame(&mut self.av_frame) {
                Ok(_) if self.av_frame.has_decode_errors() => {
                    // Catches damage that was not signaled with LostData. Frames referencing this
                    // one are not flagged, so wait for the next recovery point.
                    debug!("H264 decoder produced a corrupted frame. Dropping frame.");
                    let pts = self.av_frame.pts().map(Timestamp::from_nanos);
                    self.au_splitter.wait_for_recovery_point(pts);
                    if let Some(s) = &self.keyframe_request_sender {
                        s.send()
                    }
                }
                Ok(_) => match from_av_frame(&mut self.av_frame) {
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
