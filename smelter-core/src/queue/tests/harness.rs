//! Test harness for queue tests.
//!
//! The queue is driven by the real clock, so tests run in real time: a scenario
//! spanning 200ms of media takes about 200ms. Tests block on output channels
//! with a timeout instead of mocking time; when a scenario needs to "wait", just
//! `std::thread::sleep`.
//!
//! Typical test structure:
//! - `TestQueue::new` + `add_input` (before or after `start`, depending on scenario)
//! - send frames/samples via `TestInput` (or `stream_video_then_eos` for sends
//!   that must not block the test thread, e.g. before queue start)
//! - `start()` the queue
//! - read output with `next_video_batch`/`next_audio_batch` and compare against
//!   expected `VideoBatch`/`AudioBatch` values; all PTS in summaries are relative
//!   to the queue start, so expectations are deterministic for `Pts`/`FromStart`
//!   offsets. For `QueueTrackOffset::None` the offset depends on wall clock —
//!   assert with tolerance instead of equality.

#![allow(dead_code)]

use std::{collections::HashMap, sync::Arc, thread, time::Duration};

use bytes::Bytes;
use crossbeam_channel::{Receiver, Sender, unbounded};
use smelter_render::{Frame, FrameData, Framerate, InputId, Resolution};

use crate::{
    event::{Event, EventEmitter},
    prelude::*,
    queue::{
        DEFAULT_AUDIO_CHUNK_DURATION, Queue, QueueAudioOutput, QueueContext, QueueInput,
        QueueInputOptions, QueueOptions, QueueSender, QueueTrackOptions, QueueVideoOutput,
    },
    types::Ref,
};

/// Distance between queue creation and start to desync clocks
pub const OFFSET: Duration = Duration::from_micros(123_456);

pub const OUTPUT_FRAMERATE: Framerate = Framerate { num: 50, den: 1 };
/// Duration of a single video batch at [`OUTPUT_FRAMERATE`]; equal to the audio
/// chunk duration, so video and audio batches line up 1:1.
pub const BATCH_DURATION: Duration = DEFAULT_AUDIO_CHUNK_DURATION;

pub fn ms(value: u64) -> Duration {
    Duration::from_millis(value)
}

#[derive(Debug, Clone)]
pub struct TestQueueOptions {
    pub output_framerate: Framerate,
    pub ahead_of_time_processing: bool,
    pub run_late_scheduled_events: bool,
    pub never_drop_output_frames: bool,
    /// Use a zero-capacity video output channel: the queue drops non-required
    /// batches whose PTS deadline passes before the test reads them (with the
    /// default unbounded channel every batch is recorded).
    pub bounded_video_output: bool,
}

impl Default for TestQueueOptions {
    fn default() -> Self {
        Self {
            output_framerate: OUTPUT_FRAMERATE,
            ahead_of_time_processing: false,
            run_late_scheduled_events: false,
            never_drop_output_frames: false,
            bounded_video_output: false,
        }
    }
}

/// Single frame of a video batch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputFrame {
    Frame {
        /// Identifies the source frame: n-th video frame sent on this input.
        id: u32,
        /// PTS relative to queue start.
        pts: Duration,
    },
    Eos,
}

/// Summary of [`QueueVideoOutput`] with PTS relative to queue start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoBatch {
    pub pts: Duration,
    pub required: bool,
    pub frames: HashMap<InputId, InputFrame>,
}

/// Samples from a single input in an audio batch, PTS ranges relative to queue start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputSamples {
    Batches(Vec<(Duration, Duration)>),
    Eos,
}

/// Summary of [`QueueAudioOutput`] with PTS relative to queue start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioBatch {
    pub start_pts: Duration,
    pub end_pts: Duration,
    pub required: bool,
    pub samples: HashMap<InputId, InputSamples>,
}

/// Build the `frames` map of an expected [`VideoBatch`].
pub fn frames<const N: usize>(frames: [(&str, InputFrame); N]) -> HashMap<InputId, InputFrame> {
    frames
        .into_iter()
        .map(|(id, frame)| (InputId(id.into()), frame))
        .collect()
}

/// Assert that batches are exactly equal.
#[track_caller]
pub fn assert_video_batch_eq(actual: &VideoBatch, expected: &VideoBatch) {
    assert_eq!(actual, expected);
}

/// Assert that the batch was produced at `pts` with no input delivering anything.
#[track_caller]
pub fn assert_empty_video_batch(actual: &VideoBatch, pts: Duration) {
    assert_eq!(
        actual,
        &VideoBatch {
            pts,
            required: false,
            frames: frames([]),
        }
    );
}

/// Like [`assert_video_batch_eq`], but compares frame PTS with `pts_tolerance`.
/// Batch PTS, ids and required flags are still compared exactly; tolerance is
/// only for frame PTS that depend on the real clock (offsets resolved relative
/// to `sync_point` or initialized on the first received frame).
#[track_caller]
pub fn assert_video_batch_eq_with_tolerance(
    actual: &VideoBatch,
    expected: &VideoBatch,
    pts_tolerance: Duration,
) {
    let frame_matches =
        |actual: Option<&InputFrame>, expected: &InputFrame| match (actual, expected) {
            (Some(InputFrame::Eos), InputFrame::Eos) => true,
            (
                Some(InputFrame::Frame { id, pts }),
                InputFrame::Frame {
                    id: expected_id,
                    pts: expected_pts,
                },
            ) => id == expected_id && pts.abs_diff(*expected_pts) <= pts_tolerance,
            _ => false,
        };
    assert!(
        actual.pts == expected.pts
            && actual.required == expected.required
            && actual.frames.len() == expected.frames.len()
            && expected
                .frames
                .iter()
                .all(|(input_id, frame)| frame_matches(actual.frames.get(input_id), frame)),
        "batches don't match (PTS tolerance {pts_tolerance:?})\nactual: {actual:#?}\nexpected: {expected:#?}",
    );
}

/// Build the `samples` map of an expected [`AudioBatch`].
pub fn samples<const N: usize>(
    samples: [(&str, InputSamples); N],
) -> HashMap<InputId, InputSamples> {
    samples
        .into_iter()
        .map(|(id, samples)| (InputId(id.into()), samples))
        .collect()
}

/// Audio variant of [`assert_video_batch_eq`]: chunk PTS ranges and required
/// flags are compared exactly, sample batch PTS ranges with `pts_tolerance`.
#[track_caller]
pub fn assert_audio_batch_eq(actual: &AudioBatch, expected: &AudioBatch, pts_tolerance: Duration) {
    if pts_tolerance.is_zero() {
        assert_eq!(actual, expected);
        return;
    }
    let samples_match =
        |actual: Option<&InputSamples>, expected: &InputSamples| match (actual, expected) {
            (Some(InputSamples::Eos), InputSamples::Eos) => true,
            (Some(InputSamples::Batches(actual)), InputSamples::Batches(expected)) => {
                actual.len() == expected.len()
                    && actual
                        .iter()
                        .zip(expected)
                        .all(|((a_start, a_end), (e_start, e_end))| {
                            a_start.abs_diff(*e_start) <= pts_tolerance
                                && a_end.abs_diff(*e_end) <= pts_tolerance
                        })
            }
            _ => false,
        };
    assert!(
        actual.start_pts == expected.start_pts
            && actual.end_pts == expected.end_pts
            && actual.required == expected.required
            && actual.samples.len() == expected.samples.len()
            && expected
                .samples
                .iter()
                .all(|(input_id, samples)| samples_match(actual.samples.get(input_id), samples)),
        "batches don't match (PTS tolerance {pts_tolerance:?})\nactual: {actual:#?}\nexpected: {expected:#?}",
    );
}

pub struct TestQueue {
    pub queue: Arc<Queue>,
    queue_ctx: QueueContext,
    event_emitter: Arc<EventEmitter>,
    events: Receiver<Event>,
    video_receiver: Receiver<QueueVideoOutput>,
    audio_receiver: Receiver<QueueAudioOutput>,
    video_sender: Option<Sender<QueueVideoOutput>>,
    audio_sender: Option<Sender<QueueAudioOutput>>,
}

impl TestQueue {
    pub fn new(opts: TestQueueOptions) -> Self {
        // ignore the error when a subscriber is already set (multiple tests
        // in one process)
        let _ = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::TRACE)
            .with_test_writer()
            .try_init();
        let queue = Queue::new(QueueOptions {
            output_framerate: opts.output_framerate,
            ahead_of_time_processing: opts.ahead_of_time_processing,
            run_late_scheduled_events: opts.run_late_scheduled_events,
            never_drop_output_frames: opts.never_drop_output_frames,
            side_channel_socket_dir: None,
            tick_duration: Duration::from_micros(50),
        });
        let event_emitter = Arc::new(EventEmitter::new());
        let events = event_emitter.subscribe();
        let queue_ctx = queue.ctx();
        // Queue output is paced by the queue itself against the real clock;
        // unbounded channels just record everything it produces.
        let (video_sender, video_receiver) = match opts.bounded_video_output {
            true => crossbeam_channel::bounded(0),
            false => unbounded(),
        };
        let (audio_sender, audio_receiver) = unbounded();
        Self {
            queue,
            queue_ctx,
            event_emitter,
            events,
            video_receiver,
            audio_receiver,
            video_sender: Some(video_sender),
            audio_sender: Some(audio_sender),
        }
    }

    /// Register an input and create its first track.
    pub fn add_input(
        &self,
        input_id: &str,
        opts: QueueInputOptions,
        track: QueueTrackOptions,
    ) -> TestInput {
        let input_id = InputId(input_id.into());
        let input_ref = Ref::new(&input_id);
        let queue_input = QueueInput::new_inner(
            self.queue_ctx.clone(),
            self.event_emitter.clone(),
            &input_ref,
            opts,
            None,
            None,
        );
        let (video, audio) = queue_input.queue_new_track(track);
        self.queue.add_input(&input_id, queue_input.clone());
        TestInput {
            input_id,
            queue_input,
            video,
            audio,
            next_frame_id: 0,
        }
    }

    /// Start the queue. All PTS values in batch summaries are relative to this moment.
    pub fn start(&mut self) {
        self.queue.start(
            self.video_sender.take().expect("queue already started"),
            self.audio_sender.take().expect("queue already started"),
        );
    }

    fn start_pts(&self) -> Duration {
        self.queue_ctx.start_pts.value().expect("queue not started")
    }

    /// Next video batch if one was already produced.
    pub fn next_video_batch(&self) -> Option<VideoBatch> {
        let batch = self.video_receiver.try_recv().ok()?;
        Some(self.summarize_video(batch))
    }

    /// Next audio batch if one was already produced.
    pub fn next_audio_batch(&self) -> Option<AudioBatch> {
        let batch = self.audio_receiver.try_recv().ok()?;
        Some(self.summarize_audio(batch))
    }

    /// Events emitted by queue inputs so far (delivered/playing/paused/EOS).
    pub fn drain_events(&self) -> Vec<Event> {
        self.events.try_iter().collect()
    }

    /// Assert that exactly `expected` events were emitted since the last
    /// `expect_events`/`drain_events` call.
    #[track_caller]
    pub fn expect_events(&self, expected: &[Event]) {
        let actual = self.drain_events();
        // Event does not implement PartialEq, compare debug representations
        assert_eq!(format!("{actual:?}"), format!("{expected:?}"));
    }

    /// Like `expect_events`, but ignores the order. Use when events from
    /// different tracks fire at tick-timing-dependent moments.
    #[track_caller]
    pub fn expect_events_unordered(&self, expected: &[Event]) {
        let mut actual: Vec<String> = self
            .drain_events()
            .iter()
            .map(|event| format!("{event:?}"))
            .collect();
        let mut expected: Vec<String> = expected.iter().map(|event| format!("{event:?}")).collect();
        actual.sort();
        expected.sort();
        assert_eq!(actual, expected);
    }

    fn summarize_video(&self, batch: QueueVideoOutput) -> VideoBatch {
        let start_pts = self.start_pts();
        VideoBatch {
            pts: batch.pts.saturating_sub(start_pts),
            required: batch.required,
            frames: batch
                .frames
                .into_iter()
                .map(|(id, event)| {
                    let event = match event {
                        PipelineEvent::Data(frame) => InputFrame::Frame {
                            id: test_frame_id(&frame),
                            pts: frame.pts.saturating_sub(start_pts),
                        },
                        PipelineEvent::EOS => InputFrame::Eos,
                    };
                    (id, event)
                })
                .collect(),
        }
    }

    fn summarize_audio(&self, batch: QueueAudioOutput) -> AudioBatch {
        let start_pts = self.start_pts();
        AudioBatch {
            start_pts: batch.start_pts.saturating_sub(start_pts),
            end_pts: batch.end_pts.saturating_sub(start_pts),
            required: batch.required,
            samples: batch
                .samples
                .into_iter()
                .map(|(id, event)| {
                    let event = match event {
                        PipelineEvent::Data(batches) => InputSamples::Batches(
                            batches
                                .iter()
                                .map(|batch| {
                                    (
                                        batch.start_pts.saturating_sub(start_pts),
                                        batch.end_pts().saturating_sub(start_pts),
                                    )
                                })
                                .collect(),
                        ),
                        PipelineEvent::EOS => InputSamples::Eos,
                    };
                    (id, event)
                })
                .collect(),
        }
    }
}

impl Drop for TestQueue {
    fn drop(&mut self) {
        self.queue.shutdown();
    }
}

pub struct TestInput {
    pub input_id: InputId,
    pub queue_input: QueueInput,
    video: Option<QueueSender<Frame>>,
    audio: Option<QueueSender<InputAudioSamples>>,
    next_frame_id: u32,
}

impl TestInput {
    /// Queue a new track on this input, replacing the track senders. The queue
    /// switches to it once the current track ends (or immediately after
    /// `queue_input.abort_old_track()`). Frame ids keep incrementing across
    /// tracks.
    pub fn new_track(&mut self, track: QueueTrackOptions) {
        let (video, audio) = self.queue_input.queue_new_track(track);
        self.video = video;
        self.audio = audio;
    }

    /// Send a single frame and return its id (n-th video frame sent on this input).
    /// Blocks on queue backpressure (the queue buffers ~100ms of input plus one
    /// frame in the channel).
    pub fn send_frame(&mut self, pts: Duration) -> u32 {
        let id = self.next_frame_id;
        self.next_frame_id += 1;
        self.video
            .as_ref()
            .expect("video track not active")
            .send(test_frame(id, pts))
            .expect("video channel closed");
        id
    }

    /// Close the video track; the queue emits EOS once buffered frames drain.
    pub fn end_video(&mut self) {
        self.video.take().expect("video track not active");
    }

    /// Send frames from a background thread and close the video track afterwards.
    /// Use when sends would block the test thread, e.g. before the queue starts.
    pub fn stream_video_then_eos(&mut self, frame_pts: Vec<Duration>) -> thread::JoinHandle<()> {
        let sender = self.video.take().expect("video track not active");
        let first_id = self.next_frame_id;
        self.next_frame_id += frame_pts.len() as u32;
        thread::spawn(move || {
            for (index, pts) in frame_pts.into_iter().enumerate() {
                if sender
                    .send(test_frame(first_id + index as u32, pts))
                    .is_err()
                {
                    return;
                }
            }
        })
    }

    /// Send a batch of silence. Blocks on queue backpressure.
    pub fn send_samples(&self, start_pts: Duration, duration: Duration) {
        self.audio
            .as_ref()
            .expect("audio track not active")
            .send(test_samples(start_pts, duration))
            .expect("audio channel closed");
    }

    /// Send `count` batches of silence starting at `first_pts`, back to back.
    pub fn send_sample_batches(&self, first_pts: Duration, duration: Duration, count: u32) {
        for index in 0..count {
            self.send_samples(first_pts + duration * index, duration);
        }
    }

    /// Close the audio track; the queue emits EOS once buffered samples drain.
    pub fn end_audio(&mut self) {
        self.audio.take().expect("audio track not active");
    }

    /// Send sample batches from a background thread and close the audio track
    /// afterwards. Use when sends would block the test thread, e.g. before the
    /// queue starts.
    pub fn stream_audio_then_eos(
        &mut self,
        batch_pts: Vec<Duration>,
        duration: Duration,
    ) -> thread::JoinHandle<()> {
        let sender = self.audio.take().expect("audio track not active");
        thread::spawn(move || {
            for pts in batch_pts {
                if sender.send(test_samples(pts, duration)).is_err() {
                    return;
                }
            }
        })
    }

    pub fn video_delivered_event(&self) -> Event {
        Event::VideoInputStreamDelivered(self.input_id.clone())
    }

    pub fn video_playing_event(&self) -> Event {
        Event::VideoInputStreamPlaying(self.input_id.clone())
    }

    pub fn video_eos_event(&self) -> Event {
        Event::VideoInputStreamEos(self.input_id.clone())
    }

    pub fn video_paused_event(&self) -> Event {
        Event::VideoInputStreamPaused(self.input_id.clone())
    }

    pub fn audio_paused_event(&self) -> Event {
        Event::AudioInputStreamPaused(self.input_id.clone())
    }

    pub fn audio_delivered_event(&self) -> Event {
        Event::AudioInputStreamDelivered(self.input_id.clone())
    }

    pub fn audio_playing_event(&self) -> Event {
        Event::AudioInputStreamPlaying(self.input_id.clone())
    }

    pub fn audio_eos_event(&self) -> Event {
        Event::AudioInputStreamEos(self.input_id.clone())
    }
}

/// 1x1 BGRA frame with `id` encoded in the pixel data, so output frames can be
/// matched back to the frames a test sent.
pub fn test_frame(id: u32, pts: Duration) -> Frame {
    Frame {
        data: FrameData::Bgra(Bytes::copy_from_slice(&id.to_le_bytes())),
        resolution: Resolution {
            width: 1,
            height: 1,
        },
        pts,
    }
}

fn test_frame_id(frame: &Frame) -> u32 {
    match &frame.data {
        FrameData::Bgra(data) => u32::from_le_bytes(data[..].try_into().unwrap()),
        data => panic!("expected a frame created with test_frame, got {data:?}"),
    }
}

pub fn test_samples(start_pts: Duration, duration: Duration) -> InputAudioSamples {
    const SAMPLE_RATE: u32 = 48_000;
    let sample_count = (duration.as_secs_f64() * SAMPLE_RATE as f64).round() as usize;
    InputAudioSamples::new(
        AudioSamples::Mono(vec![0.0; sample_count]),
        start_pts,
        SAMPLE_RATE,
    )
}
