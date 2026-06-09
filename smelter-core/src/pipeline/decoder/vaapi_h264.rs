#[cfg(all(feature = "vaapi", target_os = "linux"))]
mod imp {
    use std::{sync::Arc, time::Duration};

    use gpu_video::vaapi::h264::WgpuTexturesDecoder;
    use smelter_render::{Frame, FrameData, Resolution};
    use tracing::{debug, info, trace, warn};

    use crate::{
        pipeline::decoder::{
            EncodedInputEvent, KeyframeRequestSender, VideoDecoder, VideoDecoderInstance,
        },
        prelude::*,
    };

    pub struct VaapiH264Decoder {
        decoder: WgpuTexturesDecoder,
        keyframe_request_sender: Option<KeyframeRequestSender>,
    }

    impl VideoDecoder for VaapiH264Decoder {
        const LABEL: &'static str = "VA-API H264 decoder";

        fn new(
            ctx: &Arc<PipelineCtx>,
            keyframe_request_sender: Option<KeyframeRequestSender>,
        ) -> Result<Self, DecoderInitError> {
            info!("Initializing VA-API H264 decoder");
            let adapter_info = ctx.graphics_context.adapter.get_info();
            let decoder = WgpuTexturesDecoder::new(
                Arc::clone(&ctx.graphics_context.device),
                Arc::clone(&ctx.graphics_context.queue),
                Some(&adapter_info),
            )
            .map_err(|err| {
                DecoderInitError::VaapiH264DecoderUnavailable(err.to_string())
            })?;
            Ok(Self { decoder, keyframe_request_sender })
        }
    }

    impl VideoDecoderInstance for VaapiH264Decoder {
        fn decode(&mut self, event: EncodedInputEvent) -> Vec<Frame> {
            trace!(?event, "VA-API H264 decoder received an event.");
            let result = match event {
                EncodedInputEvent::Chunk(chunk) => {
                    if MediaKind::Video(VideoCodec::H264) != chunk.kind {
                        warn!(
                            "VA-API H264 decoder received unsupported kind {:?}",
                            chunk.kind
                        );
                        return Vec::new();
                    }
                    self.decoder.decode_chunk(
                        &chunk.data,
                        Some(duration_micros(chunk.pts)),
                        chunk.present,
                    )
                }
                EncodedInputEvent::LostData => {
                    self.decoder.mark_missed_frames();
                    self.request_keyframe();
                    return Vec::new();
                }
                EncodedInputEvent::AuDelimiter => self.decoder.flush_frame(),
            };

            match result {
                Ok(frames) => frames.into_iter().map(from_va_frame).collect(),
                Err(err) => {
                    self.request_keyframe();
                    debug!("VA-API H264 parser/decode error: {err}");
                    Vec::new()
                }
            }
        }

        fn flush(&mut self) -> Vec<Frame> {
            match self.decoder.flush() {
                Ok(frames) => frames.into_iter().map(from_va_frame).collect(),
                Err(err) => {
                    warn!("Failed to flush VA-API H264 decoder: {err}");
                    Vec::new()
                }
            }
        }
    }

    impl VaapiH264Decoder {
        fn request_keyframe(&self) {
            if let Some(sender) = self.keyframe_request_sender.as_ref() {
                sender.send();
            }
        }
    }

    fn from_va_frame(frame: gpu_video::OutputFrame<wgpu::Texture>) -> Frame {
        let gpu_video::OutputFrame { data, metadata } = frame;
        let resolution = Resolution {
            width: data.width() as usize,
            height: data.height() as usize,
        };

        Frame {
            data: FrameData::Nv12WgpuTexture(data.into()),
            pts: Duration::from_micros(metadata.pts.unwrap()),
            resolution,
        }
    }

    fn duration_micros(duration: Duration) -> u64 {
        duration.as_micros().try_into().unwrap_or(u64::MAX)
    }
}

#[cfg(all(feature = "vaapi", target_os = "linux"))]
pub use imp::VaapiH264Decoder;

#[cfg(not(all(feature = "vaapi", target_os = "linux")))]
mod imp {
    use std::sync::Arc;

    use smelter_render::Frame;

    use crate::{
        pipeline::decoder::{
            EncodedInputEvent, KeyframeRequestSender, VideoDecoder, VideoDecoderInstance,
        },
        prelude::*,
    };

    pub struct VaapiH264Decoder;

    impl VideoDecoder for VaapiH264Decoder {
        const LABEL: &'static str = "VA-API H264 decoder";

        fn new(
            _ctx: &Arc<PipelineCtx>,
            _keyframe_request_sender: Option<KeyframeRequestSender>,
        ) -> Result<Self, DecoderInitError> {
            Err(DecoderInitError::VaapiH264DecoderUnavailable(
                "support was not compiled into smelter-core".into(),
            ))
        }
    }

    impl VideoDecoderInstance for VaapiH264Decoder {
        fn decode(&mut self, _chunk: EncodedInputEvent) -> Vec<Frame> {
            Vec::new()
        }

        fn flush(&mut self) -> Vec<Frame> {
            Vec::new()
        }
    }
}

#[cfg(not(all(feature = "vaapi", target_os = "linux")))]
pub use imp::VaapiH264Decoder;
