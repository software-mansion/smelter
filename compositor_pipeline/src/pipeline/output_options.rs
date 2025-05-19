use std::sync::Arc;

use compositor_render::{scene::Component, OutputId};
use crossbeam_channel::{bounded, Receiver};

use crate::{error::RegisterOutputError, AudioScene, RegisterOutputOptions, VideoScene};

use super::{
    encoder::Encoder,
    output::{mp4::Mp4FileWriter, Output},
    EncoderOutputEvent, PipelineCtx, Port, RawDataReceiver,
};

pub(super) trait OutputOptionsExt {
    fn initial_video(&self) -> Option<VideoScene>;
    fn initial_audio(&self) -> Option<AudioScene>;
}

impl OutputOptionsExt<Option<Port>> for RegisterOutputOptions {
    fn initial_video(&self) -> Option<VideoScene> {
        match self {
            Self::Rtp(rtp) => rtp.video.map(|video| video.initial),
            Self::Rtmp(rtmp) => rtmp.video.map(|video| video.initial),
            Self::Mp4(mp4) => mp4.video.map(|video| video.initial),
            Self::Whip(whip) => whip.video.map(|video| video.initial),
        }
    }

    fn initial_audio(&self) -> Option<AudioScene> {
        match self {
            Self::Rtp(rtp) => rtp.audio.map(|audio| audio.initial),
            Self::Rtmp(rtmp) => rtmp.audio.map(|audio| audio.initial),
            Self::Mp4(mp4) => mp4.audio.map(|audio| audio.initial),
            Self::Whip(whip) => whip.audio.map(|audio| audio.initial),
        }
    }
}

pub(super) type BuildOutputFn<Options: OutputOptionsExt, OutputResult> =
    fn(
        ctx: Arc<PipelineCtx>,
        output_id: &OutputId,
        options: &Options,
    ) -> Result<(Output, OutputResult), RegisterOutputOptions>;

pub(super) fn new_output(
    ctx: Arc<PipelineCtx>,
    output_id: &OutputId,
    options: &RegisterOutputOptions,
) -> Result<(Output, Option<Port>), RegisterOutputError> {
    match options {
        RegisterOutputOptions::Rtp(rtp_options) => {
            let encoder_opts = EncoderOptions {
                video: rtp_options.video.clone(),
                audio: rtp_options.audio.clone(),
            };

            let (encoder, packets) = Encoder::new(output_id, encoder_opts, &ctx)
                .map_err(|e| RegisterOutputError::EncoderError(output_id.clone(), e))?;
            let (sender, port) = rtp::RtpSender::new(output_id, rtp_options.clone(), packets, ctx)
                .map_err(|e| RegisterOutputError::OutputError(output_id.clone(), e))?;

            Ok((Output::Rtp { sender, encoder }, Some(port)))
        }
        RegisterOutputOptions::Rtmp(rtmp_options) => {
            let encoder_opts = EncoderOptions {
                video: rtmp_options.video.clone(),
                audio: rtmp_options.audio.clone(),
            };

            let (encoder, packets) = Encoder::new(output_id, encoder_opts, &ctx)
                .map_err(|e| RegisterOutputError::EncoderError(output_id.clone(), e))?;
            let sender =
                rtmp::RmtpSender::new(output_id, rtmp_options.clone(), packets, encoder.context())
                    .map_err(|e| RegisterOutputError::OutputError(output_id.clone(), e))?;

            Ok((Output::Rtmp { sender, encoder }, None))
        }
        RegisterOutputOptions::Mp4(mp4_opt) => {
            let encoder_opts = EncoderOptions {
                video: mp4_opt.video.clone(),
                audio: mp4_opt.audio.clone(),
            };

            let (encoder, packets) = Encoder::new(output_id, encoder_opts, &ctx)
                .map_err(|e| RegisterOutputError::EncoderError(output_id.clone(), e))?;
            let writer = Mp4FileWriter::new(
                output_id.clone(),
                mp4_opt.clone(),
                encoder.context(),
                packets,
                ctx,
            )
            .map_err(|e| RegisterOutputError::OutputError(output_id.clone(), e))?;

            Ok((Output::Mp4 { writer, encoder }, None))
        }
        RegisterOutputOptions::Whip(whip_options) => {
            let (sender, encoder) = whip::WhipSender::new(output_id, whip_options.clone(), ctx)
                .map_err(|e| RegisterOutputError::OutputError(output_id.clone(), e))?;

            Ok((Output::Whip { sender, encoder }, None))
        }
    }
}

pub(super) fn new_encoded_data_output(
    ctx: Arc<PipelineCtx>,
    output_id: &OutputId,
    options: &EncodedDataOutputOptions,
) -> Result<(Output, Receiver<EncoderOutputEvent>), RegisterOutputError> {
    let encoder_opts = EncoderOptions {
        video: options.video.clone(),
        audio: options.audio.clone(),
    };

    let (encoder, packets) = Encoder::new(output_id, encoder_opts, &ctx)
        .map_err(|e| RegisterOutputError::EncoderError(output_id.clone(), e))?;

    Ok((Output::EncodedData { encoder }, packets))
}

pub(super) fn new_raw_data_output(
    ctx: Arc<PipelineCtx>,
    output_id: &OutputId,
    options: &RawDataOutputOptions,
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
