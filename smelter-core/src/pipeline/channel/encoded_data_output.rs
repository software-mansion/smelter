use std::sync::Arc;

use crossbeam_channel::bounded;
use smelter_render::OutputId;

use crate::{
    pipeline::{
        encoder::{
            encoder_thread_audio::{
                AudioEncoderThread, AudioEncoderThreadHandle, AudioEncoderThreadOptions,
            },
            encoder_thread_video::{
                VideoEncoderThread, VideoEncoderThreadHandle, VideoEncoderThreadOptions,
            },
            fdk_aac::FdkAacEncoder,
            ffmpeg_h264::FfmpegH264Encoder,
            ffmpeg_vp8::FfmpegVp8Encoder,
            ffmpeg_vp9::FfmpegVp9Encoder,
            libopus::OpusEncoder,
            vulkan_h264::VulkanH264Encoder,
        },
        output::{Output, OutputAudio, OutputVideo},
    },
    utils::InitializableThread,
};

use crate::prelude::*;

pub struct EncodedDataOutput {
    pub audio: Option<AudioEncoderThreadHandle>,
    pub video: Option<VideoEncoderThreadHandle>,
}

impl EncodedDataOutput {
    pub fn new(
        ctx: Arc<PipelineCtx>,
        output_id: Ref<OutputId>,
        options: EncodedDataOutputOptions,
    ) -> Result<(Self, EncodedDataOutputHandle), OutputInitError> {
        let (sender, encoded_chunks_receiver) = bounded(1);
        let video = match &options.video {
            Some(video) => match video {
                VideoEncoderOptions::FfmpegH264(options) => {
                    Some(VideoEncoderThread::<FfmpegH264Encoder>::spawn(
                        output_id.clone(),
                        VideoEncoderThreadOptions {
                            ctx: ctx.clone(),
                            encoder_options: options.clone(),
                            chunks_sender: sender.clone(),
                        },
                    )?)
                }
                VideoEncoderOptions::FfmpegVp8(options) => {
                    Some(VideoEncoderThread::<FfmpegVp8Encoder>::spawn(
                        output_id.clone(),
                        VideoEncoderThreadOptions {
                            ctx: ctx.clone(),
                            encoder_options: options.clone(),
                            chunks_sender: sender.clone(),
                        },
                    )?)
                }
                VideoEncoderOptions::FfmpegVp9(options) => {
                    Some(VideoEncoderThread::<FfmpegVp9Encoder>::spawn(
                        output_id.clone(),
                        VideoEncoderThreadOptions {
                            ctx: ctx.clone(),
                            encoder_options: options.clone(),
                            chunks_sender: sender.clone(),
                        },
                    )?)
                }
                VideoEncoderOptions::VulkanH264(options) => {
                    Some(VideoEncoderThread::<VulkanH264Encoder>::spawn(
                        output_id.clone(),
                        VideoEncoderThreadOptions {
                            ctx: ctx.clone(),
                            encoder_options: options.clone(),
                            chunks_sender: sender.clone(),
                        },
                    )?)
                }
            },
            None => None,
        };

        let audio = match &options.audio {
            Some(audio) => match audio {
                AudioEncoderOptions::Opus(options) => {
                    Some(AudioEncoderThread::<OpusEncoder>::spawn(
                        output_id.clone(),
                        AudioEncoderThreadOptions {
                            ctx: ctx.clone(),
                            encoder_options: options.clone(),
                            chunks_sender: sender.clone(),
                        },
                    )?)
                }
                AudioEncoderOptions::FdkAac(options) => {
                    Some(AudioEncoderThread::<FdkAacEncoder>::spawn(
                        output_id.clone(),
                        AudioEncoderThreadOptions {
                            ctx: ctx.clone(),
                            encoder_options: options.clone(),
                            chunks_sender: sender.clone(),
                        },
                    )?)
                }
            },
            None => None,
        };

        let handle = EncodedDataOutputHandle {
            receiver: encoded_chunks_receiver,
            video: video.as_ref().map(|v| VideoEncoderInfo {
                resolution: v.config.resolution,
                extradata: v.config.extradata.clone(),
            }),
            audio: audio.as_ref().map(|a| AudioEncoderInfo {
                extradata: a.config.extradata.clone(),
            }),
        };
        Ok((Self { video, audio }, handle))
    }
}

impl Output for EncodedDataOutput {
    fn audio(&self) -> Option<OutputAudio<'_>> {
        self.audio.as_ref().map(|audio| OutputAudio {
            samples_batch_sender: &audio.sample_batch_sender,
        })
    }

    fn video(&self) -> Option<OutputVideo<'_>> {
        self.video.as_ref().map(|video| OutputVideo {
            resolution: video.config.resolution,
            frame_format: video.config.output_format,
            frame_sender: &video.frame_sender,
            keyframe_request_sender: &video.keyframe_request_sender,
        })
    }

    fn kind(&self) -> OutputProtocolKind {
        OutputProtocolKind::EncodedDataChannel
    }
}
