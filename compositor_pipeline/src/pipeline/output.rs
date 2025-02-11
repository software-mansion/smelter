use std::sync::Arc;

use compositor_render::{
    error::RequestKeyframeError, Frame, OutputFrameFormat, OutputId, Resolution,
};
use crossbeam_channel::{bounded, Receiver, Sender};
use mp4::{Mp4FileWriter, Mp4OutputOptions};
use rtmp::RtmpSenderOptions;
use tracing::debug;

use crate::{audio_mixer::OutputSamples, error::RegisterOutputError, queue::PipelineEvent};

use self::rtp::{RtpSender, RtpSenderOptions};

use super::{
    encoder::{AudioEncoderOptions, Encoder, EncoderOptions, VideoEncoderOptions},
    types::EncoderOutputEvent,
    PipelineCtx, Port, RawDataReceiver,
};
use whip::{WhipSender, WhipSenderOptions};

pub mod mp4;
pub mod rtmp;
pub mod rtp;
pub mod whip;

/// Options to configure public outputs that can be constructed via REST API
#[derive(Debug, Clone)]
pub struct OutputOptions {
    pub output_protocol: OutputProtocolOptions,
    pub video: Option<VideoEncoderOptions>,
    pub audio: Option<AudioEncoderOptions>,
}

#[derive(Debug, Clone)]
pub enum OutputProtocolOptions {
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
    Rtp {
        sender: RtpSender,
        encoder: Encoder,
    },
    Rtmp {
        sender: rtmp::RmtpSender,
        encoder: Encoder,
    },
    Mp4 {
        writer: Mp4FileWriter,
        encoder: Encoder,
    },
    Whip {
        sender: WhipSender,
        encoder: Encoder,
    },
    EncodedData {
        encoder: Encoder,
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
        output_id: &OutputId,
        ctx: Arc<PipelineCtx>,
    ) -> Result<(Output, NewOutputResult), RegisterOutputError>;
}

impl OutputOptionsExt<Option<Port>> for OutputOptions {
    fn new_output(
        &self,
        output_id: &OutputId,
        ctx: Arc<PipelineCtx>,
    ) -> Result<(Output, Option<Port>), RegisterOutputError> {
        let encoder_opts = EncoderOptions {
            video: self.video.clone(),
            audio: self.audio.clone(),
        };

        let (encoder, packets) = Encoder::new(output_id, encoder_opts, ctx.mixing_sample_rate)
            .map_err(|e| RegisterOutputError::EncoderError(output_id.clone(), e))?;

        match &self.output_protocol {
            OutputProtocolOptions::Rtp(rtp_options) => {
                let (sender, port) =
                    rtp::RtpSender::new(output_id, rtp_options.clone(), packets, ctx)
                        .map_err(|e| RegisterOutputError::OutputError(output_id.clone(), e))?;

                Ok((Output::Rtp { sender, encoder }, Some(port)))
            }
            OutputProtocolOptions::Rtmp(rtmp_options) => {
                let sender = rtmp::RmtpSender::new(output_id, rtmp_options.clone(), packets)
                    .map_err(|e| RegisterOutputError::OutputError(output_id.clone(), e))?;

                Ok((Output::Rtmp { sender, encoder }, None))
            }
            OutputProtocolOptions::Mp4(mp4_opt) => {
                let writer = Mp4FileWriter::new(output_id.clone(), mp4_opt.clone(), packets, ctx)
                    .map_err(|e| RegisterOutputError::OutputError(output_id.clone(), e))?;

                Ok((Output::Mp4 { writer, encoder }, None))
            }
            OutputProtocolOptions::Whip(whip_options) => {
                let sender = whip::WhipSender::new(
                    output_id,
                    whip_options.clone(),
                    packets,
                    encoder.keyframe_request_sender(),
                    ctx,
                )
                .map_err(|e| RegisterOutputError::OutputError(output_id.clone(), e))?;

                Ok((Output::Whip { sender, encoder }, None))
            }
        }
    }
}

impl OutputOptionsExt<Receiver<EncoderOutputEvent>> for EncodedDataOutputOptions {
    fn new_output(
        &self,
        output_id: &OutputId,
        ctx: Arc<PipelineCtx>,
    ) -> Result<(Output, Receiver<EncoderOutputEvent>), RegisterOutputError> {
        let encoder_opts = EncoderOptions {
            video: self.video.clone(),
            audio: self.audio.clone(),
        };

        let (encoder, packets) = Encoder::new(output_id, encoder_opts, ctx.mixing_sample_rate)
            .map_err(|e| RegisterOutputError::EncoderError(output_id.clone(), e))?;

        Ok((Output::EncodedData { encoder }, packets))
    }
}

impl OutputOptionsExt<RawDataReceiver> for RawDataOutputOptions {
    fn new_output(
        &self,
        _output_id: &OutputId,
        _ctx: Arc<PipelineCtx>,
    ) -> Result<(Output, RawDataReceiver), RegisterOutputError> {
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
            Output::Rtp { encoder, .. } => encoder.frame_sender(),
            Output::Rtmp { encoder, .. } => encoder.frame_sender(),
            Output::Mp4 { encoder, .. } => encoder.frame_sender(),
            Output::Whip { encoder, .. } => encoder.frame_sender(),
            Output::EncodedData { encoder } => encoder.frame_sender(),
            Output::RawData { video, .. } => video.as_ref(),
        }
    }

    pub fn samples_batch_sender(&self) -> Option<&Sender<PipelineEvent<OutputSamples>>> {
        match &self {
            Output::Rtp { encoder, .. } => encoder.samples_batch_sender(),
            Output::Rtmp { encoder, .. } => encoder.samples_batch_sender(),
            Output::Mp4 { encoder, .. } => encoder.samples_batch_sender(),
            Output::Whip { encoder, .. } => encoder.samples_batch_sender(),
            Output::EncodedData { encoder } => encoder.samples_batch_sender(),
            Output::RawData { audio, .. } => audio.as_ref(),
        }
    }

    pub fn resolution(&self) -> Option<Resolution> {
        match &self {
            Output::Rtp { encoder, .. } => encoder.video.as_ref().map(|v| v.resolution()),
            Output::Rtmp { encoder, .. } => encoder.video.as_ref().map(|v| v.resolution()),
            Output::Mp4 { encoder, .. } => encoder.video.as_ref().map(|v| v.resolution()),
            Output::Whip { encoder, .. } => encoder.video.as_ref().map(|v| v.resolution()),
            Output::EncodedData { encoder } => encoder.video.as_ref().map(|v| v.resolution()),
            Output::RawData { resolution, .. } => *resolution,
        }
    }

    pub fn request_keyframe(&self, output_id: OutputId) -> Result<(), RequestKeyframeError> {
        let encoder = match &self {
            Output::Rtp { encoder, .. } => encoder,
            Output::Rtmp { encoder, .. } => encoder,
            Output::Mp4 { encoder, .. } => encoder,
            Output::Whip { encoder, .. } => encoder,
            Output::EncodedData { encoder } => encoder,
            Output::RawData { .. } => return Err(RequestKeyframeError::RawOutput(output_id)),
        };

        if encoder
            .video
            .as_ref()
            .ok_or(RequestKeyframeError::NoVideoOutput(output_id))?
            .keyframe_request_sender()
            .send(())
            .is_err()
        {
            debug!("Failed to send keyframe request to the encoder. Channel closed.");
        };

        Ok(())
    }

    pub(super) fn output_frame_format(&self) -> Option<OutputFrameFormat> {
        match &self {
            Output::Rtp { encoder, .. } => encoder
                .video
                .as_ref()
                .map(|_| OutputFrameFormat::PlanarYuv420Bytes),
            Output::Rtmp { encoder, .. } => encoder
                .video
                .as_ref()
                .map(|_| OutputFrameFormat::PlanarYuv420Bytes),
            Output::EncodedData { encoder } => encoder
                .video
                .as_ref()
                .map(|_| OutputFrameFormat::PlanarYuv420Bytes),
            Output::RawData { video, .. } => {
                video.as_ref().map(|_| OutputFrameFormat::RgbaWgpuTexture)
            }
            Output::Mp4 { encoder, .. } => encoder
                .video
                .as_ref()
                .map(|_| OutputFrameFormat::PlanarYuv420Bytes),
            Output::Whip { encoder, .. } => encoder
                .video
                .as_ref()
                .map(|_| OutputFrameFormat::PlanarYuv420Bytes),
        }
    }
}
