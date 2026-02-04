use std::sync::Arc;

use rtmp::{self, VideoConfig, VideoData};
use tracing::{error, info, warn};

use crate::{
    pipeline::{
        decoder::{ffmpeg_h264, vulkan_h264},
        rtmp::rtmp_input::{
            decoder_thread::{VideoDecoderThread, VideoDecoderThreadOptions},
            input_state::RtmpInputsState,
            stream_state::RtmpStreamState,
        },
        utils::{H264AvcDecoderConfig, H264AvccToAnnexB},
    },
    prelude::*,
    thread_utils::InitializableThread,
};

pub(super) fn process_video_config(
    ctx: &Arc<PipelineCtx>,
    inputs: &RtmpInputsState,
    input_ref: &Ref<InputId>,
    config: VideoConfig,
) {
    if config.codec != rtmp::VideoCodec::H264 {
        warn!(?config.codec, "Unsupported video codec");
        return;
    }

    match H264AvcDecoderConfig::parse(config.data) {
        Ok(parsed_config) => {
            info!("H264 config received");
            init_h264_decoder(ctx, inputs, input_ref, parsed_config);
        }
        Err(err) => {
            warn!(?err, "Failed to parse H264 config");
        }
    }
}

pub(super) fn process_video(
    inputs: &RtmpInputsState,
    input_ref: &Ref<InputId>,
    stream_state: &mut RtmpStreamState,
    video: VideoData,
) {
    if video.codec != rtmp::VideoCodec::H264 {
        warn!(?video.codec, "Unsupported video codec");
        return;
    }

    let Ok(Some(sender)) = inputs.video_chunk_sender(input_ref) else {
        warn!("Missing H264 decoder, skipping video until config arrives");
        return;
    };

    let (pts, dts) = stream_state.pts_dts_from_timestamps(video.pts, video.dts);

    let chunk = EncodedInputChunk {
        data: video.data,
        pts,
        dts,
        kind: MediaKind::Video(VideoCodec::H264),
    };

    if sender.send(PipelineEvent::Data(chunk)).is_err() {
        warn!("Video decoder channel closed");
    }
}

fn init_h264_decoder(
    ctx: &Arc<PipelineCtx>,
    inputs: &RtmpInputsState,
    input_ref: &Ref<InputId>,
    h264_config: H264AvcDecoderConfig,
) {
    let input_state = match inputs.get(input_ref) {
        Ok(state) => state,
        Err(err) => {
            error!(?err, "Input state missing for video decoder init");
            return;
        }
    };

    let transformer = H264AvccToAnnexB::new(h264_config);
    let decoder_thread_options = VideoDecoderThreadOptions {
        ctx: ctx.clone(),
        transformer: Some(transformer),
        frame_sender: input_state.frame_sender.clone(),
        input_buffer_size: 10,
    };

    let vulkan_supported = ctx.graphics_context.has_vulkan_decoder_support();
    let h264_decoder = input_state.video_decoders.h264.unwrap_or({
        if vulkan_supported {
            VideoDecoderOptions::VulkanH264
        } else {
            VideoDecoderOptions::FfmpegH264
        }
    });

    let handle = match h264_decoder {
        VideoDecoderOptions::FfmpegH264 => {
            VideoDecoderThread::<ffmpeg_h264::FfmpegH264Decoder, _>::spawn(
                input_ref.clone(),
                decoder_thread_options,
            )
        }
        VideoDecoderOptions::VulkanH264 => {
            VideoDecoderThread::<vulkan_h264::VulkanH264Decoder, _>::spawn(
                input_ref.clone(),
                decoder_thread_options,
            )
        }
        _ => {
            error!("Invalid video decoder provided, expected H264");
            return;
        }
    };

    match handle {
        Ok(handle) => {
            if let Err(err) = inputs.set_video_decoder_handle(input_ref, handle) {
                error!(?err, "Failed to store H264 decoder handle in state");
            }
        }
        Err(err) => {
            error!(?err, "Failed to initialize H264 decoder");
        }
    }
}
