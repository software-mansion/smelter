use compositor_render::{
    error::RequestKeyframeError, Frame, OutputFrameFormat, OutputId, Resolution,
};
use crossbeam_channel::Sender;
use mp4::{Mp4FileWriter, Mp4OutputOptions};
use rtmp::RtmpSenderOptions;
use tracing::debug;

use crate::{audio_mixer::OutputSamples, queue::PipelineEvent, AudioChannels, MixingStrategy};

use self::rtp::{RtpSender, RtpSenderOptions};

use super::{
    encoder::{AudioEncoderOptions, Encoder, VideoEncoderOptions},
    pipeline_output::PipelineOutputEndConditionState,
};
use whip::{WhipSender, WhipSenderOptions};

pub mod mp4;
pub mod rtmp;
pub mod rtp;
pub mod whip;

pub(crate) trait Output {
    fn video(&self) -> Option<OutputVideo>;
    fn audio(&self) -> Option<OutputAudio>;
    fn request_keyframe(&self, output_id: OutputId) -> Result<(), RequestKeyframeError>;
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct OutputVideo<'a> {
    pub resolution: Resolution,
    pub frame_format: OutputFrameFormat,
    pub frame_sender: &'a Sender<PipelineEvent<Frame>>,
    pub end_condition: &'a PipelineOutputEndConditionState,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct OutputAudio<'a> {
    pub mixing_strategy: MixingStrategy,
    pub channels: AudioChannels,
    pub samples_batch_sender: &'a Sender<PipelineEvent<OutputSamples>>,
    pub end_condition: &'a PipelineOutputEndConditionState,
}

//pub enum Output {
//    Rtp {
//        sender: RtpSender,
//        encoder: Encoder,
//    },
//    Rtmp {
//        sender: rtmp::RmtpSender,
//        encoder: Encoder,
//    },
//    Mp4 {
//        writer: Mp4FileWriter,
//        encoder: Encoder,
//    },
//    Whip {
//        sender: WhipSender,
//        encoder: Encoder,
//    },
//    EncodedData {
//        encoder: Encoder,
//    },
//    RawData {
//        resolution: Option<Resolution>,
//        video: Option<Sender<PipelineEvent<Frame>>>,
//        audio: Option<Sender<PipelineEvent<OutputSamples>>>,
//    },
//}

//impl Output {
//    pub fn frame_sender(&self) -> Option<&Sender<PipelineEvent<Frame>>> {
//        match &self {
//            Output::Rtp { encoder, .. } => encoder.frame_sender(),
//            Output::Rtmp { encoder, .. } => encoder.frame_sender(),
//            Output::Mp4 { encoder, .. } => encoder.frame_sender(),
//            Output::Whip { encoder, .. } => encoder.frame_sender(),
//            Output::EncodedData { encoder } => encoder.frame_sender(),
//            Output::RawData { video, .. } => video.as_ref(),
//        }
//    }
//
//    pub fn samples_batch_sender(&self) -> Option<&Sender<PipelineEvent<OutputSamples>>> {
//        match &self {
//            Output::Rtp { encoder, .. } => encoder.samples_batch_sender(),
//            Output::Rtmp { encoder, .. } => encoder.samples_batch_sender(),
//            Output::Mp4 { encoder, .. } => encoder.samples_batch_sender(),
//            Output::Whip { encoder, .. } => encoder.samples_batch_sender(),
//            Output::EncodedData { encoder } => encoder.samples_batch_sender(),
//            Output::RawData { audio, .. } => audio.as_ref(),
//        }
//    }
//
//    pub fn resolution(&self) -> Option<Resolution> {
//        match &self {
//            Output::Rtp { encoder, .. } => encoder.video.as_ref().map(|v| v.resolution()),
//            Output::Rtmp { encoder, .. } => encoder.video.as_ref().map(|v| v.resolution()),
//            Output::Mp4 { encoder, .. } => encoder.video.as_ref().map(|v| v.resolution()),
//            Output::Whip { encoder, .. } => encoder.video.as_ref().map(|v| v.resolution()),
//            Output::EncodedData { encoder } => encoder.video.as_ref().map(|v| v.resolution()),
//            Output::RawData { resolution, .. } => *resolution,
//        }
//    }
//
//    pub fn request_keyframe(&self, output_id: OutputId) -> Result<(), RequestKeyframeError> {
//        let encoder = match &self {
//            Output::Rtp { encoder, .. } => encoder,
//            Output::Rtmp { encoder, .. } => encoder,
//            Output::Mp4 { encoder, .. } => encoder,
//            Output::Whip { encoder, .. } => encoder,
//            Output::EncodedData { encoder } => encoder,
//            Output::RawData { .. } => return Err(RequestKeyframeError::RawOutput(output_id)),
//        };
//
//        if encoder
//            .video
//            .as_ref()
//            .ok_or(RequestKeyframeError::NoVideoOutput(output_id))?
//            .keyframe_request_sender()
//            .send(())
//            .is_err()
//        {
//            debug!("Failed to send keyframe request to the encoder. Channel closed.");
//        };
//
//        Ok(())
//    }
//
//    pub(super) fn output_frame_format(&self) -> Option<OutputFrameFormat> {
//        match &self {
//            Output::Rtp { encoder, .. } => encoder
//                .video
//                .as_ref()
//                .map(|_| OutputFrameFormat::PlanarYuv420Bytes),
//            Output::Rtmp { encoder, .. } => encoder
//                .video
//                .as_ref()
//                .map(|_| OutputFrameFormat::PlanarYuv420Bytes),
//            Output::EncodedData { encoder } => encoder
//                .video
//                .as_ref()
//                .map(|_| OutputFrameFormat::PlanarYuv420Bytes),
//            Output::RawData { video, .. } => {
//                video.as_ref().map(|_| OutputFrameFormat::RgbaWgpuTexture)
//            }
//            Output::Mp4 { encoder, .. } => encoder
//                .video
//                .as_ref()
//                .map(|_| OutputFrameFormat::PlanarYuv420Bytes),
//            Output::Whip { encoder, .. } => encoder
//                .video
//                .as_ref()
//                .map(|_| OutputFrameFormat::PlanarYuv420Bytes),
//        }
//    }
//}
