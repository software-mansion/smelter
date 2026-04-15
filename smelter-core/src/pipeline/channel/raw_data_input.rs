use std::{
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use crossbeam_channel::{Sender, bounded};
use tracing::{debug, trace};

use crate::{
    pipeline::input::Input,
    queue::{QueueInput, QueueTrackOffset, QueueTrackOptions},
    utils::input_buffer::InputDelayBuffer,
};

use crate::prelude::*;

/// RawData input - receives raw video frames and audio samples via in-process channels,
/// normalizes timestamps, and feeds them into the queue.
///
/// ## Timestamps
///
/// - Queue tracks are created immediately.
/// - With offset (`opts.offset = Some(offset)`)
///   - PTS of first frame is normalized to zero (subtracts first observed PTS)
///   - Register track with `QueueTrackOffset::FromStart(offset)`
///   - There is a initial buffering phase, but it does not affect the result
///     because offset from start already establishes synchronization
/// - Without offset (`opts.offset = None`)
///   - PTS of first frame is normalized to zero (subtracts first observed PTS)
///   - Before any frame is produced it's first buffered until `options.buffer_duration`
///     is collected
///   - Register track with `QueueTrackOffset::None`
///
/// ### Unsupported scenarios
/// - If ahead of time processing is enabled, initial registration will happen on pts already
///   processed by the queue, but queue will wait and eventually stream will show up, with
///   the portion at the start cut off.
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

        let queue_input = QueueInput::new(&ctx, &input_ref, options.required);
        let (video_sender, audio_sender) = queue_input.queue_new_track(QueueTrackOptions {
            video: options.video,
            audio: options.audio,
            offset: match options.offset {
                Some(offset) => QueueTrackOffset::FromStart(offset),
                None => QueueTrackOffset::None,
            },
        });

        let first_pts = Arc::new(Mutex::new(None));

        let video_sender = video_sender.map(|frame_sender| {
            spawn_video_repacking_thread(
                &input_ref,
                first_pts.clone(),
                InputDelayBuffer::new(buffer_duration),
                frame_sender,
            )
        });
        let audio_sender = audio_sender.map(|samples_sender| {
            spawn_audio_repacking_thread(
                &input_ref,
                first_pts.clone(),
                InputDelayBuffer::new(buffer_duration),
                samples_sender,
            )
        });

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
    input_ref: &Ref<InputId>,
    first_pts: Arc<Mutex<Option<Duration>>>,
    mut buffer: InputDelayBuffer<Frame>,
    frame_sender: Sender<Frame>,
) -> Sender<PipelineEvent<Frame>> {
    let (input_sender, input_receiver) = bounded::<PipelineEvent<Frame>>(1000);

    thread::Builder::new()
        .name(format!(
            "Raw channel video synchronization thread for input {input_ref}"
        ))
        .spawn(move || {
            for event in input_receiver.into_iter() {
                match event {
                    PipelineEvent::Data(frame) => buffer.write(frame),
                    PipelineEvent::EOS => buffer.mark_end(),
                };

                while let Some(mut frame) = buffer.read() {
                    let first_pts = *first_pts.lock().unwrap().get_or_insert(frame.pts);
                    frame.pts = frame.pts.saturating_sub(first_pts);

                    trace!(?frame, "Sending raw frame");
                    if frame_sender.send(frame).is_err() {
                        debug!("Failed to send frame. Channel closed.");
                        break;
                    }
                }
                if buffer.is_done() {
                    break;
                }
            }
        })
        .unwrap();

    input_sender
}

fn spawn_audio_repacking_thread(
    input_ref: &Ref<InputId>,
    first_pts: Arc<Mutex<Option<Duration>>>,
    mut buffer: InputDelayBuffer<InputAudioSamples>,
    samples_sender: Sender<InputAudioSamples>,
) -> Sender<PipelineEvent<InputAudioSamples>> {
    let (input_sender, input_receiver) = bounded::<PipelineEvent<InputAudioSamples>>(1000);

    thread::Builder::new()
        .name(format!(
            "Raw channel audio synchronization thread for input {input_ref}"
        ))
        .spawn(move || {
            for event in input_receiver.into_iter() {
                match event {
                    PipelineEvent::Data(frame) => buffer.write(frame),
                    PipelineEvent::EOS => buffer.mark_end(),
                };

                while let Some(mut batch) = buffer.read() {
                    let first_pts = *first_pts.lock().unwrap().get_or_insert(batch.start_pts);
                    batch.start_pts = batch.start_pts.saturating_sub(first_pts);

                    trace!(?batch, "Sending raw frame");
                    if samples_sender.send(batch).is_err() {
                        debug!("Failed to send sample batch. Channel closed.");
                        break;
                    }
                }
                if buffer.is_done() {
                    break;
                }
            }
        })
        .unwrap();

    input_sender
}
