use compositor_render::{Frame, Framerate, OutputId, Resolution};
use crossbeam_channel::{bounded, Receiver, Sender};
use fdk_aac::AacEncoder;
use log::error;
use resampler::OutputResampler;

use crate::{
    audio_mixer::{AudioChannels, OutputSamples},
    error::EncoderInitError,
    queue::PipelineEvent,
};

use self::{ffmpeg_h264::LibavH264Encoder, opus::OpusEncoder};

use super::types::EncoderOutputEvent;

pub mod fdk_aac;
pub mod ffmpeg_h264;
pub mod ffmpeg_vp8;
pub mod opus;
mod resampler;

pub struct EncoderOptions {
    pub video: Option<VideoEncoderOptions>,
    pub audio: Option<AudioEncoderOptions>,
}

#[derive(Debug, Clone)]
pub enum VideoEncoderOptions {
    H264(ffmpeg_h264::Options),
}

#[derive(Debug, Clone)]
pub enum AudioEncoderOptions {
    Opus(opus::OpusEncoderOptions),
    Aac(fdk_aac::AacEncoderOptions),
}

#[derive(Debug, Clone, Copy)]
pub enum AudioEncoderPreset {
    Quality,
    Voip,
    LowestLatency,
}

pub struct Encoder {
    pub video: Option<VideoEncoder>,
    pub audio: Option<AudioEncoder>,
}

pub enum VideoEncoder {
    H264(LibavH264Encoder),
}

pub enum AudioEncoder {
    Opus(OpusEncoder),
    Aac(AacEncoder),
}

impl Encoder {
    pub fn new(
        output_id: &OutputId,
        options: EncoderOptions,
        framerate: Framerate,
        sample_rate: u32,
    ) -> Result<(Self, Receiver<EncoderOutputEvent>), EncoderInitError> {
        let (encoded_chunks_sender, encoded_chunks_receiver) = bounded(1);

        let video_encoder = match options.video {
            Some(video_encoder_options) => Some(VideoEncoder::new(
                output_id,
                video_encoder_options,
                framerate,
                encoded_chunks_sender.clone(),
            )?),
            None => None,
        };

        let audio_encoder = match options.audio {
            Some(audio_encoder_options) => Some(AudioEncoder::new(
                output_id,
                audio_encoder_options,
                sample_rate,
                encoded_chunks_sender,
            )?),
            None => None,
        };

        Ok((
            Self {
                video: video_encoder,
                audio: audio_encoder,
            },
            encoded_chunks_receiver,
        ))
    }

    pub fn frame_sender(&self) -> Option<&Sender<PipelineEvent<Frame>>> {
        match &self.video {
            Some(VideoEncoder::H264(encoder)) => Some(encoder.frame_sender()),
            None => {
                error!("Non video encoder received frame to send.");
                None
            }
        }
    }

    pub fn keyframe_request_sender(&self) -> Option<Sender<()>> {
        match self.video.as_ref() {
            Some(VideoEncoder::H264(encoder)) => Some(encoder.keyframe_request_sender().clone()),
            None => {
                error!("Non video encoder received keyframe request.");
                None
            }
        }
    }

    pub fn samples_batch_sender(&self) -> Option<&Sender<PipelineEvent<OutputSamples>>> {
        match &self.audio {
            Some(encoder) => Some(encoder.samples_batch_sender()),
            None => {
                error!("Non audio encoder received samples to send.");
                None
            }
        }
    }
}

impl VideoEncoderOptions {
    pub fn resolution(&self) -> Resolution {
        match self {
            VideoEncoderOptions::H264(opt) => opt.resolution,
        }
    }
}

impl VideoEncoder {
    pub fn new(
        output_id: &OutputId,
        options: VideoEncoderOptions,
        framerate: Framerate,
        sender: Sender<EncoderOutputEvent>,
    ) -> Result<Self, EncoderInitError> {
        match options {
            VideoEncoderOptions::H264(options) => Ok(Self::H264(LibavH264Encoder::new(
                output_id, options, framerate, sender,
            )?)),
        }
    }

    pub fn resolution(&self) -> Resolution {
        match self {
            Self::H264(encoder) => encoder.resolution(),
        }
    }

    pub fn keyframe_request_sender(&self) -> Sender<()> {
        match self {
            Self::H264(encoder) => encoder.keyframe_request_sender(),
        }
    }
}

impl AudioEncoder {
    fn new(
        output_id: &OutputId,
        options: AudioEncoderOptions,
        mixing_sample_rate: u32,
        sender: Sender<EncoderOutputEvent>,
    ) -> Result<Self, EncoderInitError> {
        let resampler = if options.sample_rate() != mixing_sample_rate {
            Some(OutputResampler::new(
                mixing_sample_rate,
                options.sample_rate(),
            )?)
        } else {
            None
        };

        match options {
            AudioEncoderOptions::Opus(options) => {
                OpusEncoder::new(options, sender, resampler).map(AudioEncoder::Opus)
            }
            AudioEncoderOptions::Aac(options) => {
                AacEncoder::new(output_id, options, sender, resampler).map(AudioEncoder::Aac)
            }
        }
    }

    fn samples_batch_sender(&self) -> &Sender<PipelineEvent<OutputSamples>> {
        match self {
            Self::Opus(encoder) => encoder.samples_batch_sender(),
            Self::Aac(encoder) => encoder.samples_batch_sender(),
        }
    }
}

impl AudioEncoderOptions {
    pub fn channels(&self) -> AudioChannels {
        match self {
            AudioEncoderOptions::Opus(options) => options.channels,
            AudioEncoderOptions::Aac(options) => options.channels,
        }
    }

    fn sample_rate(&self) -> u32 {
        match self {
            AudioEncoderOptions::Opus(options) => options.sample_rate,
            AudioEncoderOptions::Aac(options) => options.sample_rate,
        }
    }
}
