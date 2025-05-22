use std::sync::Arc;

use compositor_render::OutputId;
use crossbeam_channel::{bounded, Receiver};

use crate::{
    error::OutputInitError,
    pipeline::{
        encoder::{
            encoder_thread_audio::{spawn_audio_encoder_thread, AudioEncoderThreadHandle},
            encoder_thread_video::{spawn_video_encoder_thread, VideoEncoderThreadHandle},
            fdk_aac::FdkAacEncoder,
            ffmpeg_h264::FfmpegH264Encoder,
            ffmpeg_vp8::FfmpegVp8Encoder,
            ffmpeg_vp9::FfmpegVp9Encoder,
            opus::OpusEncoder,
            AudioEncoderOptions, VideoEncoderOptions,
        },
        EncoderOutputEvent, PipelineCtx,
    },
};

use super::{EncodedDataOutputOptions, Output, OutputAudio, OutputKind, OutputVideo};

pub(crate) struct EncodedDataOutput {
    pub audio: Option<AudioEncoderThreadHandle>,
    pub video: Option<VideoEncoderThreadHandle>,
}

impl EncodedDataOutput {
    pub fn new(
        output_id: OutputId,
        ctx: Arc<PipelineCtx>,
        options: EncodedDataOutputOptions,
    ) -> Result<(Self, Receiver<EncoderOutputEvent>), OutputInitError> {
        let (sender, encoded_chunks_receiver) = bounded(1);
        let video = match &options.video {
            Some(video) => match video {
                VideoEncoderOptions::H264(options) => {
                    Some(spawn_video_encoder_thread::<FfmpegH264Encoder>(
                        ctx.clone(),
                        output_id.clone(),
                        options.clone(),
                        sender.clone(),
                    )?)
                }
                VideoEncoderOptions::VP8(options) => {
                    Some(spawn_video_encoder_thread::<FfmpegVp8Encoder>(
                        ctx.clone(),
                        output_id.clone(),
                        options.clone(),
                        sender.clone(),
                    )?)
                }
                VideoEncoderOptions::VP9(options) => {
                    Some(spawn_video_encoder_thread::<FfmpegVp9Encoder>(
                        ctx.clone(),
                        output_id.clone(),
                        options.clone(),
                        sender.clone(),
                    )?)
                }
            },
            None => None,
        };

        let audio = match &options.audio {
            Some(audio) => match audio {
                AudioEncoderOptions::Opus(options) => {
                    Some(spawn_audio_encoder_thread::<OpusEncoder>(
                        ctx.clone(),
                        output_id.clone(),
                        options.clone(),
                        sender.clone(),
                    )?)
                }
                AudioEncoderOptions::Aac(options) => {
                    Some(spawn_audio_encoder_thread::<FdkAacEncoder>(
                        ctx.clone(),
                        output_id.clone(),
                        options.clone(),
                        sender.clone(),
                    )?)
                }
            },
            None => None,
        };

        Ok((Self { video, audio }, encoded_chunks_receiver))
    }
}

impl Output for EncodedDataOutput {
    fn audio(&self) -> Option<OutputAudio> {
        self.audio.as_ref().map(|audio| OutputAudio {
            samples_batch_sender: &audio.sample_batch_sender,
        })
    }

    fn video(&self) -> Option<OutputVideo> {
        self.video.as_ref().map(|video| OutputVideo {
            resolution: video.config.resolution,
            frame_format: video.config.output_format,
            frame_sender: &video.frame_sender,
            keyframe_request_sender: &video.keyframe_request_sender,
        })
    }

    fn kind(&self) -> OutputKind {
        OutputKind::EncodedDataChannel
    }
}
