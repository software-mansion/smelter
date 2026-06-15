use std::{sync::Arc, time::Duration};

use gpu_video::{EncodedInputChunk, H264DecoderEvent, quicksync::h264::WgpuTexturesDecoderH264};
use smelter_render::{Frame, FrameData, Resolution};
use tracing::{debug, trace, warn};

use crate::{
    pipeline::decoder::{EncodedInputEvent, KeyframeRequestSender, VideoDecoder, VideoDecoderInstance},
    prelude::*,
};

pub struct QuickSyncH264Decoder {
    decoder: WgpuTexturesDecoderH264,
    keyframe_request_sender: Option<KeyframeRequestSender>,
    drop_frames: bool,
}

const MISSING_PTS: &str = "Intel Quick Sync H264 decoded frame must carry PTS";

impl VideoDecoder for QuickSyncH264Decoder {
    const LABEL: &'static str = "Intel Quick Sync H264 decoder";

    fn new(
        ctx: &Arc<PipelineCtx>,
        keyframe_request_sender: Option<KeyframeRequestSender>,
    ) -> Result<Self, DecoderInitError> {
        let adapter_info = ctx.graphics_context.adapter.get_info();
        let decoder = WgpuTexturesDecoderH264::new(
            Arc::clone(&ctx.graphics_context.device),
            Arc::clone(&ctx.graphics_context.queue),
            &adapter_info,
        )
        .map_err(|err| DecoderInitError::QuickSyncH264DecoderUnavailable(err.to_string()))?;
        Ok(Self {
            decoder,
            keyframe_request_sender,
            drop_frames: false,
        })
    }
}

fn frames_from_gpu_video_nv12(
    frames: impl IntoIterator<Item = gpu_video::OutputFrame<wgpu::Texture>>,
) -> Vec<Frame> {
    frames
        .into_iter()
        .map(|frame| {
            let gpu_video::OutputFrame { data, metadata } = frame;
            Frame {
                resolution: Resolution {
                    width: data.width() as usize,
                    height: data.height() as usize,
                },
                data: FrameData::Nv12WgpuTexture(data.into()),
                pts: Duration::from_micros(metadata.pts.expect(MISSING_PTS)),
            }
        })
        .collect()
}

impl VideoDecoderInstance for QuickSyncH264Decoder {
    fn decode(&mut self, event: EncodedInputEvent) -> Vec<Frame> {
        trace!(?event, "Intel Quick Sync H264 decoder received an event.");

        let decoder_event = match &event {
            EncodedInputEvent::Chunk(chunk) => {
                self.drop_frames = !chunk.present;
                H264DecoderEvent::DecodeChunk(EncodedInputChunk {
                    data: chunk.data.as_ref(),
                    pts: Some(chunk.pts.as_micros() as u64),
                })
            }
            EncodedInputEvent::LostData => {
                self.request_keyframe();
                H264DecoderEvent::SignalDataLoss
            }
            EncodedInputEvent::AuDelimiter => H264DecoderEvent::SignalFrameEnd,
        };

        let frames = match self.decoder.process_event(decoder_event) {
            Ok(frames) => frames,
            Err(err) => {
                self.request_keyframe();
                debug!("Intel Quick Sync H264 parser/decode error: {err}");
                return Vec::new();
            }
        };

        if self.drop_frames {
            Vec::new()
        } else {
            frames_from_gpu_video_nv12(frames)
        }
    }

    fn flush(&mut self) -> Vec<Frame> {
        let frames = match self.decoder.flush() {
            Ok(frames) => frames,
            Err(err) => {
                warn!("Failed to flush Intel Quick Sync H264 decoder: {err}");
                return Vec::new();
            }
        };

        if self.drop_frames {
            Vec::new()
        } else {
            frames_from_gpu_video_nv12(frames)
        }
    }
}

impl QuickSyncH264Decoder {
    fn request_keyframe(&self) {
        if let Some(sender) = self.keyframe_request_sender.as_ref() {
            sender.send();
        }
    }
}
