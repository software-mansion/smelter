use std::time::Duration;

use compositor_render::{Frame, FrameData, InputId, Resolution};
use crossbeam_channel::{Receiver, Sender};
use tracing::{debug, error, span, trace, warn, Level};
use vk_video::WgpuTexturesDecoder;

use crate::{
    error::InputInitError,
    pipeline::{EncodedChunk, EncodedChunkKind, PipelineCtx, VideoCodec},
    queue::PipelineEvent,
};

pub fn start_vulkan_video_decoder_thread(
    pipeline_ctx: &PipelineCtx,
    chunks_receiver: Receiver<PipelineEvent<EncodedChunk>>,
    frame_sender: Sender<PipelineEvent<Frame>>,
    input_id: InputId,
    send_eos: bool,
) -> Result<(), InputInitError> {
    let Some(vulkan_ctx) = pipeline_ctx.vulkan_ctx.clone() else {
        return Err(InputInitError::VulkanContextRequiredForVulkanDecoder);
    };

    let decoder = vulkan_ctx.device.create_wgpu_textures_decoder()?;

    std::thread::Builder::new()
        .name(format!("h264 vulkan video decoder {}", input_id.0))
        .spawn(move || {
            let _span = span!(
                Level::INFO,
                "h264 vulkan video decoder",
                input_id = input_id.to_string()
            )
            .entered();
            run_decoder_thread(decoder, chunks_receiver, frame_sender, send_eos)
        })
        .unwrap();

    Ok(())
}

fn run_decoder_thread(
    mut decoder: WgpuTexturesDecoder,
    chunks_receiver: Receiver<PipelineEvent<EncodedChunk>>,
    frame_sender: Sender<PipelineEvent<Frame>>,
    send_eos: bool,
) {
    for chunk in chunks_receiver {
        let chunk = match chunk {
            PipelineEvent::Data(chunk) => chunk,
            PipelineEvent::EOS => {
                break;
            }
        };

        if chunk.kind != EncodedChunkKind::Video(VideoCodec::H264) {
            error!(
                "H264 decoder received chunk of wrong kind: {:?}",
                chunk.kind
            );
            continue;
        }

        let chunk = vk_video::EncodedChunk {
            data: chunk.data.as_ref(),
            pts: Some(chunk.pts.as_micros() as u64),
        };

        let result = match decoder.decode(chunk) {
            Ok(res) => res,
            Err(err) => {
                warn!("Failed to decode frame: {err}");
                continue;
            }
        };

        for vk_video::Frame { data, pts } in result {
            let resolution = Resolution {
                width: data.width() as usize,
                height: data.height() as usize,
            };

            let frame = Frame {
                data: FrameData::Nv12WgpuTexture(data.into()),
                pts: Duration::from_micros(pts.unwrap()),
                resolution,
            };

            trace!(pts=?frame.pts, "H264 decoder produced a frame.");
            if frame_sender.send(PipelineEvent::Data(frame)).is_err() {
                debug!("Failed to send frame from H264 decoder. Channel closed.");
                return;
            }
        }
    }
    if send_eos && frame_sender.send(PipelineEvent::EOS).is_err() {
        debug!("Failed to send EOS from H264 decoder. Channel closed.")
    }
}
