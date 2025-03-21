use std::{sync::Arc, time::Duration};

use compositor_render::{Frame, FrameData, InputId, Resolution};
use crossbeam_channel::{Receiver, Sender};
use tracing::{debug, error, span, trace, warn, Level};
use vk_video::VulkanDevice;

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

    let (init_result_sender, init_result_receiver) = crossbeam_channel::bounded(0);

    std::thread::Builder::new()
        .name(format!("h264 vulkan video decoder {}", input_id.0))
        .spawn(move || {
            let _span = span!(
                Level::INFO,
                "h264 vulkan video decoder",
                input_id = input_id.to_string()
            )
            .entered();
            run_decoder_thread(
                vulkan_ctx.device,
                init_result_sender,
                chunks_receiver,
                frame_sender,
                send_eos,
            )
        })
        .unwrap();

    init_result_receiver.recv().unwrap()?;

    Ok(())
}

fn run_decoder_thread(
    vulkan_device: Arc<VulkanDevice>,
    init_result_sender: Sender<Result<(), InputInitError>>,
    chunks_receiver: Receiver<PipelineEvent<EncodedChunk>>,
    frame_sender: Sender<PipelineEvent<Frame>>,
    send_eos: bool,
) {
    let mut decoder = match vulkan_device.create_wgpu_textures_decoder() {
        Ok(decoder) => {
            init_result_sender.send(Ok(())).unwrap();
            decoder
        }
        Err(err) => {
            init_result_sender.send(Err(err.into())).unwrap();
            return;
        }
    };

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

        let result = match decoder.decode(&chunk.data, Some(chunk.pts.as_micros() as u64)) {
            Ok(res) => res,
            Err(err) => {
                warn!("Failed to decode frame: {err}");
                continue;
            }
        };

        for vk_video::Frame { frame, pts } in result {
            let resolution = Resolution {
                width: frame.width() as usize,
                height: frame.height() as usize,
            };

            let frame = Frame {
                data: FrameData::Nv12WgpuTexture(frame.into()),
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
