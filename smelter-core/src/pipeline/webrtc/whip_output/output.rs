use std::{sync::Arc, time::Duration};

use tokio::{sync::mpsc, time::timeout};
use tracing::{Instrument, Level, span};
use webrtc::track::track_local::track_local_static_rtp::TrackLocalStaticRTP;

use crate::{
    pipeline::webrtc::whip_output::{
        stream_media_to_peer::WhipClientTask, track_task_audio::WhipAudioTrackThreadHandle,
        track_task_video::WhipVideoTrackThreadHandle,
    },
    prelude::*,
};

use crate::pipeline::{
    output::{Output, OutputAudio, OutputVideo},
    rtp::RtpPacket,
};

/// WHIP output - pushes media to a remote WHIP server.
///
/// ## Codec negotiation
///
/// This side creates the SDP offer from encoder preferences. For H.264 encoders
/// (FFmpeg and Vulkan), the offer includes constrained baseline 3.1 (for Twitch
/// compatibility) and constrained baseline, main, and high profiles at level
/// 5.1. After receiving the answer, we determine which codec was negotiated and
/// select the matching encoder.
#[derive(Debug)]
pub(crate) struct WhipOutput {
    pub video: Option<WhipVideoTrackThreadHandle>,
    pub audio: Option<WhipAudioTrackThreadHandle>,
}

const WHIP_INIT_TIMEOUT: Duration = Duration::from_secs(60);

impl WhipOutput {
    pub fn new(
        ctx: Arc<PipelineCtx>,
        output_ref: Ref<OutputId>,
        options: WhipOutputOptions,
    ) -> Result<Self, OutputInitError> {
        ctx.stats_sender.send(StatsEvent::NewOutput {
            output_ref: output_ref.clone(),
            kind: OutputProtocolKind::Whip,
        });

        let span = span!(
            Level::INFO,
            "WHIP client task",
            output_id = output_ref.to_string()
        );
        let init_result = ctx.block_on_async_task(|ctx| async {
            // timeout has to be created inside tokio runtime
            timeout(
                WHIP_INIT_TIMEOUT,
                WhipClientTask::new(ctx, output_ref, options),
            )
            .instrument(span.clone())
            .await
        });
        let (whip_client_task, output) = match init_result {
            Ok(result) => result.map_err(|err| OutputInitError::WhipInitError(err.into()))?,
            Err(_) => return Err(OutputInitError::WhipInitTimeout),
        };

        ctx.tokio_rt.spawn(whip_client_task.run().instrument(span));

        Ok(output)
    }
}

pub(super) struct WhipClientTrack {
    pub receiver: mpsc::Receiver<RtpPacket>,
    pub track: Arc<TrackLocalStaticRTP>,
}

impl Output for WhipOutput {
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
        OutputProtocolKind::Whip
    }
}

pub(super) struct WhipOutputStatsSender {
    stats_sender: StatsSender,
    output_ref: Ref<OutputId>,
}

impl WhipOutputStatsSender {
    pub fn new(stats_sender: StatsSender, output_ref: Ref<OutputId>) -> Self {
        Self {
            stats_sender,
            output_ref,
        }
    }

    pub fn bytes_sent_event(&self, size: usize, track_kind: StatsTrackKind) {
        self.stats_sender.send(
            WhipOutputTrackStatsEvent::BytesSent(size).into_event(&self.output_ref, track_kind),
        );
    }
}
