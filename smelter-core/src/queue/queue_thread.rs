use std::{
    collections::BTreeMap,
    ops::Add,
    sync::{Arc, atomic::Ordering},
    thread::{self, JoinHandle},
    time::Duration,
};

use crossbeam_channel::{Receiver, Sender, select, tick};
use tracing::{debug, info, info_span, trace, warn};

use super::{Queue, QueueAudioOutput, QueueVideoOutput, ScheduledEvent};

pub(super) struct QueueThread {
    queue: Arc<Queue>,
    start_receiver: Receiver<QueueStartEvent>,
    scheduled_event_receiver: Receiver<ScheduledEvent>,
    scheduled_events: BTreeMap<Duration, Vec<Box<dyn FnOnce() + Send>>>,
}

pub(super) struct QueueStartEvent {
    pub video_sender: Sender<QueueVideoOutput>,
    pub audio_sender: Sender<QueueAudioOutput>,
    pub start_time_pts: Duration,
}

impl QueueThread {
    pub fn new(
        queue: Arc<Queue>,
        start_receiver: Receiver<QueueStartEvent>,
        scheduled_event_receiver: Receiver<ScheduledEvent>,
    ) -> Self {
        Self {
            queue,
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
        let ticker = tick(self.queue.output_framerate.get_interval_duration());
        while !self.queue.should_close.load(Ordering::Relaxed) {
            select! {
                recv(ticker) -> _ => {
                    self.cleanup_old_data()
                },
                recv(self.scheduled_event_receiver) -> event => {
                    let event = event.unwrap();
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
                    QueueThreadAfterStart::new(self, start_event.unwrap()).run();
                    return;
                },
            };
        }
    }

    fn cleanup_old_data(&mut self) {
        // Drop old frames as if start was happening now.
        self.queue
            .video_queue
            .lock()
            .unwrap()
            .drop_old_frames_before_start();
        self.queue
            .audio_queue
            .lock()
            .unwrap()
            .drop_old_samples_before_start()
    }
}

struct QueueThreadAfterStart {
    queue: Arc<Queue>,
    queue_start_pts: Duration,
    audio_processor: AudioQueueProcessor,
    video_processor: VideoQueueProcessor,
    scheduled_event_receiver: Receiver<ScheduledEvent>,
    scheduled_events: BTreeMap<Duration, Vec<Box<dyn FnOnce() + Send>>>,
}

impl QueueThreadAfterStart {
    fn new(queue_thread: QueueThread, start_event: QueueStartEvent) -> Self {
        Self {
            queue: queue_thread.queue.clone(),
            queue_start_pts: start_event.start_time_pts,
            audio_processor: AudioQueueProcessor {
                queue: queue_thread.queue.clone(),
                sender: start_event.audio_sender,
                chunks_counter: 0,
                queue_start_pts: start_event.start_time_pts,
            },
            video_processor: VideoQueueProcessor {
                queue: queue_thread.queue,
                sender: start_event.video_sender,
                sent_batches_counter: 0,
                queue_start_pts: start_event.start_time_pts,
            },
            scheduled_event_receiver: queue_thread.scheduled_event_receiver,
            scheduled_events: queue_thread.scheduled_events,
        }
    }

    fn run(mut self) {
        let ticker = tick(Duration::from_millis(10));

        while !self.queue.should_close.load(Ordering::Relaxed) {
            select! {
                recv(ticker) -> _ => {
                    self.on_handle_tick()
                },
                recv(self.scheduled_event_receiver) -> event => {
                    self.on_enqueue_event(event.unwrap())
                }
            };
        }
    }

    fn on_handle_tick(&mut self) {
        while !self.queue.should_close.load(Ordering::Relaxed) {
            let audio_pts_range = self.audio_processor.next_buffer_pts_range();
            let video_pts = self.video_processor.next_buffer_pts();
            let event_pts = self
                .scheduled_events
                .first_key_value()
                .map(|(pts, _)| *pts + self.queue_start_pts);

            if let Some(true) = event_pts
                .map(|event_pts: Duration| event_pts < video_pts && event_pts < audio_pts_range.0)
            {
                info!("Handle scheduled event for PTS={:?}", event_pts);
                if let Some((_, callbacks)) = self.scheduled_events.pop_first() {
                    for callback in callbacks {
                        callback()
                    }
                }
            } else if video_pts > audio_pts_range.0 {
                trace!(pts_range=?audio_pts_range, "Try to push audio samples for.");
                if self
                    .audio_processor
                    .try_push_next_sample_batch(audio_pts_range)
                    .is_none()
                {
                    break;
                }
            } else {
                trace!(pts=?video_pts, "Try to push video frames.");
                if self
                    .video_processor
                    .try_push_next_frame_set(video_pts)
                    .is_none()
                {
                    break;
                }
            }
        }
    }

    fn on_enqueue_event(&mut self, scheduled_event: ScheduledEvent) {
        let audio_pts_range = self.audio_processor.next_buffer_pts_range();
        let video_pts = self.video_processor.next_buffer_pts();
        let event_pts = self
            .scheduled_events
            .first_key_value()
            .map(|(pts, _)| *pts + self.queue_start_pts);

        let new_event_pts = scheduled_event.pts + self.queue_start_pts;

        let is_future_event = new_event_pts >= video_pts
            && new_event_pts >= audio_pts_range.0
            && new_event_pts >= event_pts.unwrap_or(Duration::ZERO);

        if self.queue.run_late_scheduled_events || is_future_event {
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
    queue: Arc<Queue>,
    sent_batches_counter: u32,
    queue_start_pts: Duration,
    sender: Sender<QueueVideoOutput>,
}

impl VideoQueueProcessor {
    fn next_buffer_pts(&self) -> Duration {
        Duration::from_secs_f64(
            self.sent_batches_counter as f64 * self.queue.output_framerate.den as f64
                / self.queue.output_framerate.num as f64,
        ) + self.queue_start_pts
    }

    /// Some(()) - Successfully pushed new frame (or dropped it).
    /// None - Nothing to push.
    fn try_push_next_frame_set(&mut self, next_buffer_pts: Duration) -> Option<()> {
        let mut internal_queue = self.queue.video_queue.lock().unwrap();

        let should_push_next_frame =
            internal_queue.should_push_next_frameset(next_buffer_pts, self.queue_start_pts);
        if !should_push_next_frame {
            return None;
        }

        let mut frames_batch =
            internal_queue.get_frames_batch(next_buffer_pts, self.queue_start_pts);
        drop(internal_queue);

        frames_batch.required = frames_batch.required || self.queue.never_drop_output_frames;

        // potentially infinitely blocking if output is not consumed
        // and one of the stream is "required"
        self.send_output_frames(frames_batch);

        Some(())
    }

    fn send_output_frames(&mut self, frameset: QueueVideoOutput) {
        let pts = frameset.pts;
        debug!(?frameset, "Pushing video frames.");
        if frameset.required {
            if self.sender.send(frameset).is_err() {
                warn!(?pts, "Dropping video frame on queue output.");
            }
        } else {
            let send_deadline = self.queue.sync_point.add(frameset.pts);
            if self.sender.send_deadline(frameset, send_deadline).is_err() {
                warn!(?pts, "Dropping video frame on queue output.");
            }
        }
        self.sent_batches_counter += 1
    }
}

struct AudioQueueProcessor {
    queue: Arc<Queue>,
    chunks_counter: u32,
    queue_start_pts: Duration,
    sender: Sender<QueueAudioOutput>,
}

impl AudioQueueProcessor {
    fn next_buffer_pts_range(&self) -> (Duration, Duration) {
        (
            self.queue_start_pts + (self.queue.audio_chunk_duration * self.chunks_counter),
            self.queue_start_pts + (self.queue.audio_chunk_duration * (self.chunks_counter + 1)),
        )
    }

    /// Some(()) - Successfully pushed new batch (or dropped it).
    /// None - Nothing to push.
    fn try_push_next_sample_batch(
        &mut self,
        next_buffer_pts_range: (Duration, Duration),
    ) -> Option<()> {
        let mut internal_queue = self.queue.audio_queue.lock().unwrap();

        let should_push_next_batch =
            internal_queue.should_push_for_pts_range(next_buffer_pts_range, self.queue_start_pts);
        if !should_push_next_batch {
            return None;
        }

        let mut samples =
            internal_queue.pop_samples_set(next_buffer_pts_range, self.queue_start_pts);
        drop(internal_queue);

        samples.required = samples.required || self.queue.never_drop_output_frames;

        self.send_output_batch(samples);

        Some(())
    }

    fn send_output_batch(&mut self, samples: QueueAudioOutput) {
        let pts_range = (samples.start_pts, samples.end_pts);
        debug!(?samples, "Pushing audio samples.");
        if samples.required {
            if self.sender.send(samples).is_err() {
                warn!(?pts_range, "Dropping audio batch on queue output.");
            }
        } else {
            let deadline = self.queue.sync_point.add(samples.start_pts);
            if self.sender.send_deadline(samples, deadline).is_err() {
                warn!(?pts_range, "Dropping audio batch on queue output.")
            }
        }
        self.chunks_counter += 1;
    }
}
