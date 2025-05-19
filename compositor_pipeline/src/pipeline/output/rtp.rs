use compositor_render::{OutputFrameFormat, OutputId};
use crossbeam_channel::bounded;
use rtp_sender::RtpSender;
use std::sync::Arc;

use crate::{
    encoder::{AudioEncoder, VideoEncoder},
    error::OutputInitError,
    pipeline::{pipeline_output::PipelineOutputEndConditionState, PipelineCtx, Port},
    MixingStrategy, PipelineEvent, RtpOutputOptions,
};

use super::{Output, OutputAudio, OutputVideo};

mod packet_stream;
mod payloader;
mod rtp_sender;
mod tcp_server;
mod udp;

struct RtpOutput {
    rtp_sender: RtpSender,
    video: Option<RtpVideoTrack>,
    audio: Option<RtpAudioTrack>,
}

struct RtpVideoTrack {
    encoder: VideoEncoder,
    end_condition: PipelineOutputEndConditionState,
}

struct RtpAudioTrack {
    mixing_strategy: MixingStrategy,
    encoder: AudioEncoder,
    end_condition: PipelineOutputEndConditionState,
}

impl RtpOutput {
    pub fn new(
        output_id: &OutputId,
        options: RtpOutputOptions,
        ctx: &Arc<PipelineCtx>,
    ) -> Result<(Self, Port), OutputInitError> {
        let (encoded_sender, encoded_receiver) = bounded(1);

        let video = match options.video {
            Some(video) => Some(RtpVideoTrack {
                encoder: VideoEncoder::new(
                    output_id,
                    video.encoder.into(),
                    ctx,
                    encoded_sender.clone(),
                )?,
                end_condition: PipelineOutputEndConditionState::new_video(video.end_condition),
            }),
            None => None,
        };

        let audio = match options.audio {
            Some(audio) => Some(RtpAudioTrack {
                mixing_strategy: audio.mixing_strategy,
                encoder: AudioEncoder::new(
                    output_id,
                    audio.encoder.into(),
                    ctx,
                    encoded_sender.clone(),
                )?,
                end_condition: PipelineOutputEndConditionState::new_audio(audio.end_condition),
            }),
            None => None,
        };

        let rtp_sender = RtpSender::new(output_id, options, encoded_receiver, ctx)?;

        Ok((
            Self {
                rtp_sender,
                video,
                audio,
            },
            port,
        ))
    }
}

impl Output for RtpOutput {
    fn video(&self) -> Option<OutputVideo> {
        self.video.map(|video| OutputVideo {
            resolution: video.encoder.resolution(),
            frame_format: OutputFrameFormat::PlanarYuv420Bytes,
            frame_sender: video.encoder.frame_sender(),
            end_condition: &video.end_condition,
        })
    }

    fn audio(&self) -> Option<OutputAudio> {
        self.audio().map(|audio| OutputAudio {
            mixing_strategy: audio.mixing_strategy,
            channels: audio.encoder.channels,
            samples_batch_sender: audio.en,
            end_condition: todo!(),
        })
    }

    fn request_keyframe(
        &self,
        output_id: OutputId,
    ) -> Result<(), compositor_render::error::RequestKeyframeError> {
        todo!()
    }
}
