//! Decoder threads of the HLS input tracks.

use std::{sync::Arc, time::Duration};

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

/// If we assume that max reasonable segment size is 10 second, then a channel
/// between the demuxer and a decoder has to fit more than one of them.
const MAX_BUFFER_SIZE: Duration = Duration::from_secs(40);

/// Everything needed to start the decoder thread of one track.
pub(super) enum TrackDecoderConfig {
    Video {
        ctx: Arc<PipelineCtx>,
        input_ref: Ref<InputId>,
        decoder: VideoDecoderOptions,
        h264_config: Option<H264AvcDecoderConfig>,
        frame_sender: QueueSender<Frame>,
    },
    Audio {
        ctx: Arc<PipelineCtx>,
        input_ref: Ref<InputId>,
        asc: Option<Bytes>,
        samples_sender: QueueSender<InputAudioSamples>,
    },
}

impl TrackDecoderConfig {
    pub(super) fn new_video(
        input: &HlsInputContext,
        extradata: Option<Bytes>,
        frame_sender: QueueSender<Frame>,
    ) -> Result<Self, InputInitError> {
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
        let decoder = match input.decoders.h264 {
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

        Ok(TrackDecoderConfig::Video {
            ctx: input.ctx.clone(),
            input_ref: input.input_ref.clone(),
            decoder,
            h264_config,
            frame_sender,
        })
    }

    pub(super) fn new_audio(
        input: &HlsInputContext,
        // not tested it was always null, but audio is in ADTS, so config is
        // not necessary
        extradata: Option<Bytes>,
        samples_sender: QueueSender<InputAudioSamples>,
    ) -> Self {
        TrackDecoderConfig::Audio {
            ctx: input.ctx.clone(),
            input_ref: input.input_ref.clone(),
            asc: extradata,
            samples_sender,
        }
    }

    /// Media the thread this spawns decodes.
    pub(super) fn kind(&self) -> MediaKind {
        match self {
            TrackDecoderConfig::Video { .. } => MediaKind::Video(VideoCodec::H264),
            TrackDecoderConfig::Audio { .. } => MediaKind::Audio(AudioCodec::Aac),
        }
    }

    pub(super) fn spawn_decoder_thread(self) -> Result<DecoderThreadHandle, InputInitError> {
        let handle = match self {
            TrackDecoderConfig::Video {
                ctx,
                input_ref,
                decoder,
                h264_config,
                frame_sender,
            } => {
                let options = VideoDecoderThreadOptions {
                    ctx,
                    transformer: h264_config.map(H264AvccToAnnexB::new),
                    frame_sender,
                    input_buffer_size: MAX_BUFFER_SIZE,
                };
                match decoder {
                    VideoDecoderOptions::FfmpegH264 => {
                        VideoDecoderThread::<ffmpeg_h264::FfmpegH264Decoder, _>::spawn(
                            input_ref, options,
                        )?
                    }
                    VideoDecoderOptions::VulkanH264 => {
                        VideoDecoderThread::<vulkan_h264::VulkanH264Decoder, _>::spawn(
                            input_ref, options,
                        )?
                    }
                    _ => {
                        return Err(InputInitError::InvalidVideoDecoderProvided {
                            expected: VideoCodec::H264,
                        });
                    }
                }
            }
            TrackDecoderConfig::Audio {
                ctx,
                input_ref,
                asc,
                samples_sender,
            } => AudioDecoderThread::<fdk_aac::FdkAacDecoder>::spawn(
                input_ref,
                AudioDecoderThreadOptions {
                    ctx,
                    decoder_options: FdkAacDecoderOptions { asc },
                    samples_sender,
                    input_buffer_size: MAX_BUFFER_SIZE,
                },
            )?,
        };
        Ok(handle)
    }
}
