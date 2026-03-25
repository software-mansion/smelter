use std::{sync::Arc, thread, time::Duration};

use crossbeam_channel::{Sender, bounded};
use tracing::{debug, trace};

use crate::{
    pipeline::input::Input,
    queue::{QueueInput, WeakQueueInput},
};

use crate::prelude::*;

pub struct RawDataInput;

impl RawDataInput {
    pub fn new_input(
        ctx: Arc<PipelineCtx>,
        input_ref: Ref<InputId>,
        options: RawDataInputOptions,
    ) -> Result<(Input, RawDataInputSender, QueueInput), InputInitError> {
        let buffer_duration = options
            .buffer_duration
            .unwrap_or(ctx.default_buffer_duration);

        let queue_input = QueueInput::new(
            options.video,
            options.audio,
            options.required,
            options.offset,
            &ctx,
            &input_ref,
        );

        let video_sender = match options.video {
            true => {
                let sender = spawn_video_repacking_thread(
                    ctx.clone(),
                    &input_ref,
                    buffer_duration,
                    queue_input.downgrade(),
                );
                Some(sender)
            }
            false => None,
        };
        let audio_sender = match options.audio {
            true => {
                let sender = spawn_audio_repacking_thread(
                    ctx.clone(),
                    &input_ref,
                    buffer_duration,
                    queue_input.downgrade(),
                );
                Some(sender)
            }
            false => None,
        };
        Ok((
            Input::RawDataChannel,
            RawDataInputSender {
                video: video_sender,
                audio: audio_sender,
            },
            queue_input,
        ))
    }
}

fn spawn_video_repacking_thread(
    ctx: Arc<PipelineCtx>,
    input_ref: &Ref<InputId>,
    buffer_duration: Duration,
    queue_input: WeakQueueInput,
) -> Sender<PipelineEvent<Frame>> {
    let (input_sender, input_receiver) = bounded::<PipelineEvent<Frame>>(1000);

    thread::Builder::new()
        .name(format!(
            "Raw channel video synchronization thread for input {input_ref}"
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
                if queue_input.send_video(event).is_err() {
                    debug!("Failed to send packet. Channel closed.");
                    break;
                }
            }
        })
        .unwrap();

    input_sender
}

fn spawn_audio_repacking_thread(
    ctx: Arc<PipelineCtx>,
    input_ref: &Ref<InputId>,
    buffer_duration: Duration,
    queue_input: WeakQueueInput,
) -> Sender<PipelineEvent<InputAudioSamples>> {
    let (input_sender, input_receiver) = bounded::<PipelineEvent<InputAudioSamples>>(1000);

    thread::Builder::new()
        .name(format!(
            "Raw channel audio synchronization thread for input {input_ref}"
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
                        PipelineEvent::Data(batch)
                    }
                    PipelineEvent::EOS => PipelineEvent::EOS,
                };
                trace!(?event, "Sending raw samples");
                if queue_input.send_audio(event).is_err() {
                    debug!("Failed to send packet. Channel closed.");
                    break;
                }
            }
        })
        .unwrap();

    input_sender
}
