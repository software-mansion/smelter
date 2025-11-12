use smelter_render::OutputId;
use std::{
    sync::Arc,
    thread,
    time::{Duration, Instant},
};
use tokio::sync::oneshot;
use tracing::{Instrument, Level, span};

use crate::{
    OutputProtocolKind, PipelineCtx,
    error::OutputInitError,
    pipeline::{
        output::{Output, OutputAudio, OutputVideo},
        webrtc::whip_output::{
            WhipClientTask, track_task_audio::WhipAudioTrackThreadHandle,
            track_task_video::WhipVideoTrackThreadHandle,
        },
    },
};

use crate::prelude::*;

#[derive(Debug)]
pub(crate) struct WhipOutput {
    pub video: Option<WhipVideoTrackThreadHandle>,
    pub audio: Option<WhipAudioTrackThreadHandle>,
}

const WHIP_INIT_TIMEOUT: Duration = Duration::from_secs(60);

impl WhipOutput {
    pub fn new(
        ctx: Arc<PipelineCtx>,
        output_id: OutputId,
        options: WhipOutputOptions,
    ) -> Result<Self, OutputInitError> {
        let (init_confirmation_sender, init_confirmation_receiver) = oneshot::channel();

        let span = span!(
            Level::INFO,
            "WHIP client task",
            output_id = output_id.to_string()
        );
        let rt = ctx.tokio_rt.clone();
        rt.spawn(
            async {
                let result = WhipClientTask::new(ctx, output_id, options).await;
                match result {
                    Ok((task, handle)) => {
                        init_confirmation_sender.send(Ok(handle)).unwrap();
                        task.run().await
                    }
                    Err(err) => init_confirmation_sender.send(Err(err)).unwrap(),
                }
            }
            .instrument(span),
        );

        wait_with_deadline(init_confirmation_receiver, WHIP_INIT_TIMEOUT)
    }
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

fn wait_with_deadline<T>(
    mut result_receiver: oneshot::Receiver<Result<T, WebrtcClientError>>,
    timeout: Duration,
) -> Result<T, OutputInitError> {
    let start_time = Instant::now();
    while start_time.elapsed() < timeout {
        thread::sleep(Duration::from_millis(500));

        match result_receiver.try_recv() {
            Ok(result) => match result {
                Ok(handle) => return Ok(handle),
                Err(err) => return Err(OutputInitError::WhipInitError(err.into())),
            },
            Err(err) => match err {
                oneshot::error::TryRecvError::Closed => {
                    return Err(OutputInitError::UnknownWhipError);
                }
                oneshot::error::TryRecvError::Empty => {}
            },
        };
    }
    result_receiver.close();
    Err(OutputInitError::WhipInitTimeout)
}
