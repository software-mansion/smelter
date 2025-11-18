use std::{slice, sync::Arc};

use bytes::Bytes;
use crossbeam_channel::Sender;
use ffmpeg_next::Stream;
use smelter_render::InputId;
use tracing::warn;

use crate::{
    pipeline::{
        decoder::{
            decoder_thread_video::{VideoDecoderThread, VideoDecoderThreadOptions},
            ffmpeg_h264,
            h264_utils::{AvccToAnnexBRepacker, H264AvcDecoderConfig},
            vulkan_h264,
        },
        rtmp::rtmp_input::{Track, stream_state::StreamState},
        utils::input_buffer::InputBuffer,
    },
    thread_utils::InitializableThread,
};

use crate::prelude::*;

pub(super) fn handle_video_track(
    ctx: &Arc<PipelineCtx>,
    input_ref: &Ref<InputId>,
    stream: &Stream<'_>,
    video_decoders: RtmpServerInputVideoDecoders,
    buffer: InputBuffer,
    frame_sender: Sender<PipelineEvent<Frame>>,
) -> Result<Track, InputInitError> {
    let state = StreamState::new(ctx.queue_sync_point, stream.time_base(), buffer);

    let extra_data = read_extra_data(stream);
    let h264_config = extra_data
        .map(H264AvcDecoderConfig::parse)
        .transpose()
        .unwrap_or_else(|e| match e {
            H264AvcDecoderConfigError::NotAVCC => None,
            _ => {
                warn!("Could not parse extra data: {e}");
                None
            }
        });

    let decoder_thread_options = VideoDecoderThreadOptions {
        ctx: ctx.clone(),
        transformer: h264_config.map(AvccToAnnexBRepacker::new),
        frame_sender,
        input_buffer_size: 2000,
    };

    let vulkan_supported = ctx.graphics_context.has_vulkan_decoder_support();
    let h264_decoder = video_decoders.h264.unwrap_or({
        match vulkan_supported {
            true => VideoDecoderOptions::VulkanH264,
            false => VideoDecoderOptions::FfmpegH264,
        }
    });

    let handle = match h264_decoder {
        VideoDecoderOptions::FfmpegH264 => {
            VideoDecoderThread::<ffmpeg_h264::FfmpegH264Decoder, _>::spawn(
                input_ref,
                decoder_thread_options,
            )?
        }
        VideoDecoderOptions::VulkanH264 => {
            if !vulkan_supported {
                return Err(InputInitError::DecoderError(
                    DecoderInitError::VulkanContextRequiredForVulkanDecoder,
                ));
            }
            VideoDecoderThread::<vulkan_h264::VulkanH264Decoder, _>::spawn(
                input_ref,
                decoder_thread_options,
            )?
        }
        _ => {
            return Err(InputInitError::InvalidVideoDecoderProvided {
                expected: VideoCodec::H264,
            });
        }
    };

    Ok(Track {
        index: stream.index(),
        handle,
        state,
    })
}

fn read_extra_data(stream: &Stream<'_>) -> Option<Bytes> {
    unsafe {
        let codecpar = (*stream.as_ptr()).codecpar;
        let size = (*codecpar).extradata_size;
        if size > 0 {
            Some(Bytes::copy_from_slice(slice::from_raw_parts(
                (*codecpar).extradata,
                size as usize,
            )))
        } else {
            None
        }
    }
}
