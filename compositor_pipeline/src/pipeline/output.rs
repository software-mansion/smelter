use std::sync::Arc;

use compositor_render::{
    error::RequestKeyframeError, Frame, OutputFrameFormat, OutputId, Resolution,
};
use crossbeam_channel::{bounded, Receiver, Sender};
use mp4::{Mp4Output, Mp4OutputOptions};
use rtmp::{RtmpClientOutput, RtmpSenderOptions};
use tracing::debug;

use crate::{audio_mixer::OutputSamples, error::OutputInitError, queue::PipelineEvent};

use self::rtp::{RtpOutput, RtpSenderOptions};

use super::{
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
    types::EncoderOutputEvent,
    PipelineCtx, Port, RawDataReceiver,
};
use whip::{WhipClientOutput, WhipSenderOptions};

pub mod mp4;
pub mod rtmp;
pub mod rtp;
pub mod whip;

#[derive(Debug, Clone)]
pub enum OutputOptions {
    Rtp(RtpSenderOptions),
    Rtmp(RtmpSenderOptions),
    Mp4(Mp4OutputOptions),
    Whip(WhipSenderOptions),
}

/// Options to configure output that sends h264 and opus audio via channel
#[derive(Debug, Clone)]
pub struct EncodedDataOutputOptions {
    pub video: Option<VideoEncoderOptions>,
    pub audio: Option<AudioEncoderOptions>,
}

/// Options to configure output that sends raw PCM audio + wgpu textures via channel
#[derive(Debug, Clone)]
pub struct RawDataOutputOptions {
    pub video: Option<RawVideoOptions>,
    pub audio: Option<RawAudioOptions>,
}

/// Options to configure audio output that returns raw video via channel.
///
/// TODO: add option, for now it implies RGBA wgpu::Texture
#[derive(Debug, Clone)]
pub struct RawVideoOptions {
    pub resolution: Resolution,
}

/// Options to configure audio output that returns raw audio via channel.
///
/// TODO: add option, for now it implies 16-bit stereo
#[derive(Debug, Clone)]
pub struct RawAudioOptions;

pub enum Output {
    Rtp(RtpOutput),
    Rtmp(RtmpClientOutput),
    Mp4(Mp4Output),
    Whip(WhipClientOutput),
    EncodedData {
        audio: Option<AudioEncoderThreadHandle>,
        video: Option<VideoEncoderThreadHandle>,
    },
    RawData {
        resolution: Option<Resolution>,
        video: Option<Sender<PipelineEvent<Frame>>>,
        audio: Option<Sender<PipelineEvent<OutputSamples>>>,
    },
}

pub(super) trait OutputOptionsExt<NewOutputResult> {
    fn new_output(
        &self,
        output_id: OutputId,
        ctx: Arc<PipelineCtx>,
    ) -> Result<(Output, NewOutputResult), OutputInitError>;
}

impl OutputOptionsExt<Option<Port>> for OutputOptions {
    fn new_output(
        &self,
        output_id: OutputId,
        ctx: Arc<PipelineCtx>,
    ) -> Result<(Output, Option<Port>), OutputInitError> {
        match &self {
            OutputOptions::Rtp(opt) => {
                let (output, port) = RtpOutput::new(ctx, output_id, opt.clone())?;
                Ok((Output::Rtp(output), Some(port)))
            }
            OutputOptions::Rtmp(opt) => {
                let output = RtmpClientOutput::new(ctx, output_id, opt.clone())?;
                Ok((Output::Rtmp(output), None))
            }
            OutputOptions::Mp4(opt) => {
                let output = Mp4Output::new(ctx, output_id, opt.clone())?;

                Ok((Output::Mp4(output), None))
            }
            OutputOptions::Whip(opt) => {
                let output = whip::WhipClientOutput::new(ctx, output_id, opt.clone())?;

                Ok((Output::Whip(output), None))
            }
        }
    }
}

impl OutputOptionsExt<Receiver<EncoderOutputEvent>> for EncodedDataOutputOptions {
    fn new_output(
        &self,
        output_id: OutputId,
        ctx: Arc<PipelineCtx>,
    ) -> Result<(Output, Receiver<EncoderOutputEvent>), OutputInitError> {
        let (sender, encoded_chunks_receiver) = bounded(1);
        let video = match &self.video {
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

        let audio = match &self.audio {
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

        Ok((
            Output::EncodedData { video, audio },
            encoded_chunks_receiver,
        ))
    }
}

impl OutputOptionsExt<RawDataReceiver> for RawDataOutputOptions {
    fn new_output(
        &self,
        _output_id: OutputId,
        _ctx: Arc<PipelineCtx>,
    ) -> Result<(Output, RawDataReceiver), OutputInitError> {
        let (video_sender, video_receiver, resolution) = match &self.video {
            Some(opts) => {
                let (sender, receiver) = bounded(100);
                (Some(sender), Some(receiver), Some(opts.resolution))
            }
            None => (None, None, None),
        };
        let (audio_sender, audio_receiver) = match self.audio {
            Some(_) => {
                let (sender, receiver) = bounded(100);
                (Some(sender), Some(receiver))
            }
            None => (None, None),
        };
        Ok((
            Output::RawData {
                resolution,
                video: video_sender,
                audio: audio_sender,
            },
            RawDataReceiver {
                video: video_receiver,
                audio: audio_receiver,
            },
        ))
    }
}

impl Output {
    pub fn frame_sender(&self) -> Option<&Sender<PipelineEvent<Frame>>> {
        match &self {
            Output::Rtp(output) => output.video.as_ref().map(|v| v.frame_sender()),
            Output::Rtmp(output) => output.video.as_ref().map(|v| v.frame_sender()),
            Output::Mp4(output) => output.video.as_ref().map(|v| v.frame_sender()),
            Output::Whip(output) => output.video.as_ref().map(|v| v.frame_sender()),
            Output::EncodedData { video, .. } => video.as_ref().map(|v| v.frame_sender()),
            Output::RawData { video, .. } => video.as_ref(),
        }
    }

    pub fn samples_batch_sender(&self) -> Option<&Sender<PipelineEvent<OutputSamples>>> {
        match &self {
            Output::Rtp(output) => output.audio.as_ref().map(|a| a.sample_batch_sender()),
            Output::Rtmp(output) => output.audio.as_ref().map(|a| a.sample_batch_sender()),
            Output::Mp4(output) => output.audio.as_ref().map(|a| a.sample_batch_sender()),
            Output::Whip(output) => output.audio.as_ref().map(|a| a.sample_batch_sender()),
            Output::EncodedData { audio, .. } => audio.as_ref().map(|a| a.sample_batch_sender()),
            Output::RawData { audio, .. } => audio.as_ref(),
        }
    }

    pub fn resolution(&self) -> Option<Resolution> {
        match &self {
            Output::Rtp(output) => output.video.as_ref().map(|v| v.resolution()),
            Output::Rtmp(output) => output.video.as_ref().map(|v| v.resolution()),
            Output::Mp4(output) => output.video.as_ref().map(|v| v.resolution()),
            Output::Whip(output) => output.video.as_ref().map(|v| v.resolution()),
            Output::EncodedData { video, .. } => video.as_ref().map(|v| v.resolution()),
            Output::RawData { resolution, .. } => *resolution,
        }
    }

    pub fn request_keyframe(&self, output_id: OutputId) -> Result<(), RequestKeyframeError> {
        let sender = match &self {
            Output::Rtp(output) => output.video.as_ref().map(|v| v.keyframe_request_sender()),
            Output::Rtmp(output) => output.video.as_ref().map(|v| v.keyframe_request_sender()),
            Output::Mp4(output) => output.video.as_ref().map(|v| v.keyframe_request_sender()),
            Output::Whip(output) => output.video.as_ref().map(|v| v.keyframe_request_sender()),
            Output::EncodedData { video, .. } => {
                video.as_ref().map(|v| v.keyframe_request_sender())
            }
            Output::RawData { .. } => return Ok(()),
        };

        if sender
            .ok_or(RequestKeyframeError::NoVideoOutput(output_id))?
            .send(())
            .is_err()
        {
            debug!("Failed to send keyframe request to the encoder. Channel closed.");
        };

        Ok(())
    }

    pub(super) fn output_frame_format(&self) -> Option<OutputFrameFormat> {
        match &self {
            Output::Rtp(output) => output.video.as_ref().map(|v| v.output_frame_format()),
            Output::Rtmp(output) => output.video.as_ref().map(|v| v.output_frame_format()),
            Output::Mp4(output) => output.video.as_ref().map(|v| v.output_frame_format()),
            Output::Whip(output) => output.video.as_ref().map(|v| v.output_frame_format()),
            Output::EncodedData { video, .. } => video.as_ref().map(|v| v.output_frame_format()),
            Output::RawData { video, .. } => {
                video.as_ref().map(|_| OutputFrameFormat::RgbaWgpuTexture)
            }
        }
    }
}
