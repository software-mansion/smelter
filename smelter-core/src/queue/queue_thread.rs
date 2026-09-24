use std::{
    collections::BTreeMap,
    ops::Add,
    sync::{Arc, Weak},
    thread::{self, JoinHandle},
    time::Duration,
};

use crossbeam_channel::{Receiver, Sender, select, tick};
use tracing::{debug, info, info_span, trace, warn};

use super::{LateEventPolicy, Queue, QueueAudioOutput, QueueVideoOutput, ScheduledEvent};
use crate::Timestamp;

pub(super) struct QueueThread {
    queue: Weak<Queue>,
    tick_duration: Duration,
    start_receiver: Receiver<QueueStartEvent>,
    scheduled_event_receiver: Receiver<ScheduledEvent>,
    scheduled_events: BTreeMap<Timestamp, Vec<Box<dyn FnOnce() + Send>>>,
}

pub(super) struct QueueStartEvent {
    pub video_sender: Sender<QueueVideoOutput>,
    pub audio_sender: Sender<QueueAudioOutput>,
    pub queue_start_pts: Timestamp,
}

impl QueueThread {
    pub fn new(
        queue: &Arc<Queue>,
        start_receiver: Receiver<QueueStartEvent>,
        scheduled_event_receiver: Receiver<ScheduledEvent>,
    ) -> Self {
        Self {
            queue: Arc::downgrade(queue),
            tick_duration: queue.tick_duration,
            start_receiver,
            scheduled_event_receiver,
            scheduled_events: BTreeMap::new(),
        }
    }

    pub fn spawn(self) -> JoinHandle<()> {
        thread::Builder::new()
            .name("Queue thread".to_string())
            .spawn(move || self.run())
            .unwrap()
    }

    fn run(mut self) {
        let _span = info_span!("Queue").entered();
        let ticker = tick(self.tick_duration);
        while self.queue.strong_count() > 0 {
            select! {
                recv(ticker) -> _ => {
                    let Some(queue) = self.queue.upgrade() else { return };
                    Self::cleanup_old_data(&queue)
                },
                recv(self.scheduled_event_receiver) -> event => {
                    // Disconnected when the queue is dropped.
                    let Ok(event) = event else { return };
                    match self.scheduled_events.get_mut(&event.pts) {
                        Some(events) => {
                            events.push(event.callback);
                        }
                        None => {
                            self.scheduled_events.insert(event.pts, vec![event.callback]);
                        }
                    }
                }
                recv(self.start_receiver) -> start_event => {
                    let Ok(start_event) = start_event else { return };
                    QueueThreadAfterStart::new(self, start_event).run();
                    return;
                },
            };
        }
    }

    fn cleanup_old_data(queue: &Queue) {
        // Drop old frames as if start was happening now.
        queue
            .video_queue
            .lock()
            .unwrap()
            .drop_old_frames_before_start();
        queue
            .audio_queue
            .lock()
            .unwrap()
            .drop_old_samples_before_start()
    }
}

struct QueueThreadAfterStart {
    queue: Weak<Queue>,
    tick_duration: Duration,
    queue_start_pts: Timestamp,
    audio_processor: AudioQueueProcessor,
    video_processor: VideoQueueProcessor,
    scheduled_event_receiver: Receiver<ScheduledEvent>,
    scheduled_events: BTreeMap<Timestamp, Vec<Box<dyn FnOnce() + Send>>>,
}

impl QueueThreadAfterStart {
    fn new(queue_thread: QueueThread, start_event: QueueStartEvent) -> Self {
        Self {
            queue: queue_thread.queue,
            tick_duration: queue_thread.tick_duration,
            queue_start_pts: start_event.queue_start_pts,
            audio_processor: AudioQueueProcessor {
                sender: start_event.audio_sender,
                chunks_counter: 0,
                queue_start_pts: start_event.queue_start_pts,
            },
            video_processor: VideoQueueProcessor {
                sender: start_event.video_sender,
                sent_batches_counter: 0,
                queue_start_pts: start_event.queue_start_pts,
            },
            scheduled_event_receiver: queue_thread.scheduled_event_receiver,
            scheduled_events: queue_thread.scheduled_events,
        }
    }

    fn run(mut self) {
        let ticker = tick(self.tick_duration);

        loop {
            select! {
                recv(ticker) -> _ => {
                    loop {
                        let Some(queue) = self.queue.upgrade() else { return };
                        if self.process_next(&queue).is_none() {
                            break;
                        }
                    }
                },
                recv(self.scheduled_event_receiver) -> event => {
                    // Disconnected when the queue is dropped.
                    let Ok(event) = event else { return };
                    let Some(queue) = self.queue.upgrade() else { return };
                    self.on_enqueue_event(&queue, event)
                }
            };
        }
    }

    /// Some(()) - Handled scheduled events or pushed (or dropped) the next batch.
    /// None - Nothing to push.
    fn process_next(&mut self, queue: &Queue) -> Option<()> {
        let audio_pts_range = self.audio_processor.next_buffer_pts_range(queue);
        let video_pts = self.video_processor.next_buffer_pts(queue);
        let event_pts = self
            .scheduled_events
            .first_key_value()
            .map(|(pts, _)| *pts + self.queue_start_pts);

        queue
            .video_queue
            .lock()
            .unwrap()
            .should_push_next_frameset(video_pts, self.queue_start_pts);

        queue
            .audio_queue
            .lock()
            .unwrap()
            .should_push_for_pts_range(audio_pts_range, self.queue_start_pts);

        if let Some(event_pts) = event_pts
            && event_pts < video_pts
            && event_pts < audio_pts_range.0
        {
            info!("Handle scheduled event for PTS={:?}", event_pts);
            queue.queue_ctx.last_pts.update(event_pts);
            if let Some((_, callbacks)) = self.scheduled_events.pop_first() {
                for callback in callbacks {
                    callback()
                }
            }
            Some(())
        } else if video_pts > audio_pts_range.0 {
            queue.queue_ctx.last_pts.update(audio_pts_range.0);
            trace!(pts_range=?audio_pts_range, "Try to push audio samples for.");
            self.audio_processor
                .try_push_next_sample_batch(queue, audio_pts_range)
        } else {
            queue.queue_ctx.last_pts.update(video_pts);
            trace!(pts=?video_pts, "Try to push video frames.");
            self.video_processor
                .try_push_next_frame_set(queue, video_pts)
        }
    }

    fn on_enqueue_event(&mut self, queue: &Queue, scheduled_event: ScheduledEvent) {
        let audio_pts_range = self.audio_processor.next_buffer_pts_range(queue);
        let video_pts = self.video_processor.next_buffer_pts(queue);
        let event_pts = self
            .scheduled_events
            .first_key_value()
            .map(|(pts, _)| *pts + self.queue_start_pts);

        let min_pts = video_pts
            .min(audio_pts_range.0)
            .min(event_pts.unwrap_or(Timestamp::MAX));

        let new_event_pts = scheduled_event.pts + self.queue_start_pts;

        let is_future_event = new_event_pts >= min_pts;
        let run_late = match scheduled_event.late_policy {
            LateEventPolicy::AlwaysRun => true,
            LateEventPolicy::Default => queue.run_late_scheduled_events,
        };

        if !is_future_event {
            warn!(
                ?new_event_pts,
                ?min_pts,
                ?run_late,
                "Scheduled event received too late"
            )
        }

        if run_late || is_future_event {
            match self.scheduled_events.get_mut(&scheduled_event.pts) {
                Some(events) => {
                    events.push(scheduled_event.callback);
                }
                None => {
                    self.scheduled_events
                        .insert(scheduled_event.pts, vec![scheduled_event.callback]);
                }
            }
        }
    }
}

struct VideoQueueProcessor {
    sent_batches_counter: u32,
    queue_start_pts: Timestamp,
    sender: Sender<QueueVideoOutput>,
}

impl VideoQueueProcessor {
    fn next_buffer_pts(&self, queue: &Queue) -> Timestamp {
        self.queue_start_pts
            + Duration::from_secs_f64(
                self.sent_batches_counter as f64 * queue.output_framerate.den as f64
                    / queue.output_framerate.num as f64,
            )
    }

    /// Some(()) - Successfully pushed new frame (or dropped it).
    /// None - Nothing to push.
    fn try_push_next_frame_set(&mut self, queue: &Queue, next_buffer_pts: Timestamp) -> Option<()> {
        let mut internal_queue = queue.video_queue.lock().unwrap();

        let should_push_next_frame =
            internal_queue.should_push_next_frameset(next_buffer_pts, self.queue_start_pts);
        if !should_push_next_frame {
            return None;
        }

        let mut frames_batch =
            internal_queue.get_frames_batch(next_buffer_pts, self.queue_start_pts);
        drop(internal_queue);

        frames_batch.required = frames_batch.required || queue.never_drop_output_frames;

        // potentially infinitely blocking if output is not consumed
        // and one of the stream is "required"
        self.send_output_frames(queue, frames_batch);

        Some(())
    }

    fn send_output_frames(&mut self, queue: &Queue, frameset: QueueVideoOutput) {
        let pts = frameset.pts;
        debug!(?pts, "Pushing video frames.");
        trace!(?frameset);
        if frameset.required {
            if self.sender.send(frameset).is_err() {
                warn!(?pts, "Dropping video frame on queue output.");
            }
        } else {
            let send_deadline = queue.queue_ctx.sync_point.add(frameset.pts);
            if self.sender.send_deadline(frameset, send_deadline).is_err() {
                warn!(?pts, "Dropping video frame on queue output.");
            }
        }
        self.sent_batches_counter += 1
    }
}

struct AudioQueueProcessor {
    chunks_counter: u32,
    queue_start_pts: Timestamp,
    sender: Sender<QueueAudioOutput>,
}

impl AudioQueueProcessor {
    fn next_buffer_pts_range(&self, queue: &Queue) -> (Timestamp, Timestamp) {
        (
            self.queue_start_pts + (queue.audio_chunk_duration * self.chunks_counter),
            self.queue_start_pts + (queue.audio_chunk_duration * (self.chunks_counter + 1)),
        )
    }

    /// Some(()) - Successfully pushed new batch (or dropped it).
    /// None - Nothing to push.
    fn try_push_next_sample_batch(
        &mut self,
        queue: &Queue,
        next_buffer_pts_range: (Timestamp, Timestamp),
    ) -> Option<()> {
        let mut internal_queue = queue.audio_queue.lock().unwrap();

        let should_push_next_batch =
            internal_queue.should_push_for_pts_range(next_buffer_pts_range, self.queue_start_pts);
        if !should_push_next_batch {
            return None;
        }

        let mut samples =
            internal_queue.pop_samples_set(next_buffer_pts_range, self.queue_start_pts);
        drop(internal_queue);

        samples.required = samples.required || queue.never_drop_output_frames;

        self.send_output_batch(queue, samples);

        Some(())
    }

    fn send_output_batch(&mut self, queue: &Queue, samples: QueueAudioOutput) {
        let pts_range = (samples.start_pts, samples.end_pts);
        debug!(?pts_range, "Pushing audio samples.");
        trace!(?samples);
        if samples.required {
            if self.sender.send(samples).is_err() {
                warn!(?pts_range, "Dropping audio batch on queue output.");
            }
        } else {
            let deadline = queue.queue_ctx.sync_point.add(samples.start_pts);
            if self.sender.send_deadline(samples, deadline).is_err() {
                warn!(?pts_range, "Dropping audio batch on queue output.")
            }
        }
        self.chunks_counter += 1;
    }
}
