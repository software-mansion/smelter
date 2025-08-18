use std::{sync::Arc, thread, time::Duration};

use crossbeam_channel::{bounded, Receiver, Sender};
use tracing::{debug, trace};

use crate::{pipeline::input::Input, queue::QueueDataReceiver};

use crate::prelude::*;

pub struct RawDataInput;

impl RawDataInput {
    pub fn new_input(
        ctx: Arc<PipelineCtx>,
        input_id: InputId,
        options: RawDataInputOptions,
    ) -> Result<(Input, RawDataInputSender, QueueDataReceiver), InputInitError> {
        let buffer_duration = options
            .buffer_duration
            .unwrap_or(ctx.default_buffer_duration);
        let (video_sender, video_receiver) = match options.video {
            true => {
                let (sender, receiver) =
                    spawn_video_repacking_thread(ctx.clone(), &input_id, buffer_duration);
                (Some(sender), Some(receiver))
            }
            false => (None, None),
        };
        let (audio_sender, audio_receiver) = match options.audio {
            true => {
                let (sender, receiver) =
                    spawn_audio_repacking_thread(ctx.clone(), &input_id, buffer_duration);
                (Some(sender), Some(receiver))
            }
            false => (None, None),
        };
        Ok((
            Input::RawDataChannel,
            RawDataInputSender {
                video: video_sender,
                audio: audio_sender,
            },
            QueueDataReceiver {
                video: video_receiver,
                audio: audio_receiver,
            },
        ))
    }
}

fn spawn_video_repacking_thread(
    ctx: Arc<PipelineCtx>,
    input_id: &InputId,
    buffer_duration: Duration,
) -> (Sender<PipelineEvent<Frame>>, Receiver<PipelineEvent<Frame>>) {
    let (output_sender, output_receiver) = bounded(5);
    let (input_sender, input_receiver) = bounded::<PipelineEvent<Frame>>(1000);

    thread::Builder::new()
        .name(format!(
            "Raw channel video synchronization thread for input {}",
            input_id
        ))
        .spawn(move || {
            let mut start_pts = None;
            let mut first_frame_pts = None;
            for event in input_receiver.into_iter() {
                let event = match event {
                    PipelineEvent::Data(mut frame) => {
                        let start_pts =
                            *start_pts.get_or_insert_with(|| ctx.queue_sync_point.elapsed());
                        let first_frame_pts = *first_frame_pts.get_or_insert(frame.pts);
                        frame.pts = frame.pts + start_pts + buffer_duration - first_frame_pts;
                        PipelineEvent::Data(frame)
                    }
                    PipelineEvent::EOS => PipelineEvent::EOS,
                };
                trace!(?event, "Sending raw frame");
                if output_sender.send(event).is_err() {
                    debug!("Failed to send packet. Channel closed.")
                }
            }
        })
        .unwrap();

    (input_sender, output_receiver)
}

fn spawn_audio_repacking_thread(
    ctx: Arc<PipelineCtx>,
    input_id: &InputId,
    buffer_duration: Duration,
) -> (
    Sender<PipelineEvent<InputAudioSamples>>,
    Receiver<PipelineEvent<InputAudioSamples>>,
) {
    let (output_sender, output_receiver) = bounded(5);
    let (input_sender, input_receiver) = bounded::<PipelineEvent<InputAudioSamples>>(1000);

    thread::Builder::new()
        .name(format!(
            "Raw channel audio synchronization thread for input {}",
            input_id
        ))
        .spawn(move || {
            let mut start_pts = None;
            let mut first_frame_pts = None;
            for event in input_receiver.into_iter() {
                let event = match event {
                    PipelineEvent::Data(mut batch) => {
                        let start_pts =
                            *start_pts.get_or_insert_with(|| ctx.queue_sync_point.elapsed());
                        let first_frame_pts = *first_frame_pts.get_or_insert(batch.start_pts);
                        batch.start_pts =
                            batch.start_pts + start_pts + buffer_duration - first_frame_pts;
                        batch.end_pts =
                            batch.end_pts + start_pts + buffer_duration - first_frame_pts;
                        PipelineEvent::Data(batch)
                    }
                    PipelineEvent::EOS => PipelineEvent::EOS,
                };
                trace!(?event, "Sending raw samples");
                if output_sender.send(event).is_err() {
                    debug!("Failed to send packet. Channel closed.")
                }
            }
        })
        .unwrap();

    (input_sender, output_receiver)
}
