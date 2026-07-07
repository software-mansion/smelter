//! Decoder threads of the HLS input tracks.

use std::time::Duration;

use bytes::Bytes;
use tracing::warn;

use crate::{
    pipeline::{
        decoder::{
            DecoderThreadHandle,
            decoder_thread_audio::{AudioDecoderThread, AudioDecoderThreadOptions},
            decoder_thread_video::{VideoDecoderThread, VideoDecoderThreadOptions},
            fdk_aac, ffmpeg_h264, vulkan_h264,
        },
        utils::{H264AvcDecoderConfig, H264AvccToAnnexB, InitializableThread},
    },
    queue::QueueSender,
};

use crate::prelude::*;

use super::hls_input::HlsInputContext;

pub(super) fn spawn_video_decoder(
    input: &HlsInputContext,
    extradata: Option<Bytes>,
    frame_sender: QueueSender<Frame>,
    buffer_size: Duration,
) -> Result<DecoderThreadHandle, InputInitError> {
    let h264_config = extradata
        .map(H264AvcDecoderConfig::parse)
        .transpose()
        .unwrap_or_else(|e| match e {
            H264AvcDecoderConfigError::NotAVCC => None,
            _ => {
                warn!("Could not parse extra data: {e}");
                None
            }
        });

    let vulkan_supported = input.ctx.graphics_context.has_vulkan_decoder_support();
    let decoder = match input.decoder_options.h264 {
        Some(VideoDecoderOptions::VulkanH264) if !vulkan_supported => {
            return Err(InputInitError::DecoderError(
                DecoderInitError::VulkanContextRequiredForVulkanDecoder,
            ));
        }
        Some(decoder) => decoder,
        None => match vulkan_supported {
            true => VideoDecoderOptions::VulkanH264,
            false => VideoDecoderOptions::FfmpegH264,
        },
    };

    let options = VideoDecoderThreadOptions {
        ctx: input.ctx.clone(),
        transformer: h264_config.map(H264AvccToAnnexB::new),
        frame_sender,
        input_buffer_size: buffer_size,
    };
    let input_ref = input.input_ref.clone();
    let handle = match decoder {
        VideoDecoderOptions::FfmpegH264 => {
            VideoDecoderThread::<ffmpeg_h264::FfmpegH264Decoder, _>::spawn(input_ref, options)?
        }
        VideoDecoderOptions::VulkanH264 => {
            VideoDecoderThread::<vulkan_h264::VulkanH264Decoder, _>::spawn(input_ref, options)?
        }
        _ => {
            return Err(InputInitError::InvalidVideoDecoderProvided {
                expected: VideoCodec::H264,
            });
        }
    };
    Ok(handle)
}

pub(super) fn spawn_audio_decoder(
    input: &HlsInputContext,
    extradata: Option<Bytes>,
    samples_sender: QueueSender<InputAudioSamples>,
    buffer_size: Duration,
) -> Result<DecoderThreadHandle, InputInitError> {
    let handle = AudioDecoderThread::<fdk_aac::FdkAacDecoder>::spawn(
        input.input_ref.clone(),
        AudioDecoderThreadOptions {
            ctx: input.ctx.clone(),
            // not tested it was always null, but audio is in ADTS, so config is
            // not necessary
            decoder_options: FdkAacDecoderOptions { asc: extradata },
            samples_sender,
            input_buffer_size: buffer_size,
        },
    )?;
    Ok(handle)
}
