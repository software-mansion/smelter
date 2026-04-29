//! Test harness for the queue.
//!
//! Provides a [`QueueHarness`] that drives the queue through its public
//! surface, backed by a virtual clock and a manually-pulsed ticker. Inputs are
//! constructed via [`QueueInput::new_for_test`] so we don't need a full
//! `PipelineCtx`. Frames and audio batches carry an `(input_idx, seq)` tag in
//! their data so tests can read identity back from any output.
//!
//! Side channels are not supported in this harness because they require
//! `wgpu_ctx` from the full pipeline. Tests covering side channels need a
//! different path or stub.

#![allow(dead_code)] // many helpers are used only by certain test groups

use std::{
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use bytes::Bytes;
use crossbeam_channel::{Receiver, Sender, bounded};
use smelter_render::{Frame, FrameData, Framerate, InputId, Resolution};

use crate::{
    event::EventEmitter,
    prelude::{InputAudioSamples, PipelineEvent},
    queue::{
        Queue, QueueAudioOutput, QueueInput, QueueOptions, QueueVideoOutput,
        clock::{Clock, SharedClock},
        queue_input::{QueueInputOptions, QueueSender, QueueTrackOffset, QueueTrackOptions},
        ticker::{SharedTicker, Ticker},
    },
    types::{AudioSamples, Ref},
};

// -- Test clock & ticker ----------------------------------------------------

/// Manually-advanced clock used by the queue test harness. The queue's
/// scheduling logic reads `Clock::now()` to decide when to emit / drop
/// frames, so tests need a `now` they can step deterministically.
#[derive(Debug, Clone)]
pub(crate) struct VirtualClock {
    inner: Arc<Mutex<Instant>>,
}

impl VirtualClock {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(Instant::now())),
        }
    }

    pub fn advance(&self, by: Duration) {
        let mut guard = self.inner.lock().unwrap();
        *guard += by;
    }
}

impl Clock for VirtualClock {
    fn now(&self) -> Instant {
        *self.inner.lock().unwrap()
    }
}

/// Ticker that only fires when [`Self::pulse`] is called. Replaces the
/// production `RealTicker` so the queue thread iterates only when the test
/// asks it to.
#[derive(Debug, Clone)]
pub(crate) struct TestTicker {
    sender: Sender<()>,
    receiver: Receiver<()>,
}

impl TestTicker {
    pub fn new() -> Self {
        // Capacity must be big enough that consecutive `pulse()` calls don't
        // block while the queue thread is busy. The queue thread drains pulses
        // promptly, so a generous bound is fine.
        let (sender, receiver) = bounded(1024);
        Self { sender, receiver }
    }

    /// Push one tick into the channel. The queue thread will run one
    /// scheduling iteration in response.
    pub fn pulse(&self) {
        // try_send so a full channel doesn't block; if the test pulses faster
        // than the queue can drain, we discard. In practice tests advance the
        // clock, then call `flush` which handles this.
        let _ = self.sender.try_send(());
    }
}

impl Ticker for TestTicker {
    fn receiver(&self) -> Receiver<()> {
        self.receiver.clone()
    }
}

// -- Tag encoding -----------------------------------------------------------

/// Pack `(input_idx, seq)` into a `u64`.
pub(crate) fn pack_tag(input_idx: u32, seq: u32) -> u64 {
    ((input_idx as u64) << 32) | (seq as u64)
}

pub(crate) fn unpack_tag(packed: u64) -> (u32, u32) {
    ((packed >> 32) as u32, (packed & 0xFFFF_FFFF) as u32)
}

/// Build a tagged 1×1 BGRA frame with the given PTS.
pub(crate) fn tagged_frame(input_idx: u32, seq: u32, pts: Duration) -> Frame {
    let mut data = Vec::with_capacity(8);
    data.extend_from_slice(&input_idx.to_le_bytes());
    data.extend_from_slice(&seq.to_le_bytes());
    Frame {
        data: FrameData::Bgra(Bytes::from(data)),
        resolution: Resolution {
            width: 1,
            height: 1,
        },
        pts,
    }
}

/// Read `(input_idx, seq)` from a frame produced by [`tagged_frame`].
pub(crate) fn read_video_tag(frame: &Frame) -> (u32, u32) {
    match &frame.data {
        FrameData::Bgra(bytes) => {
            assert_eq!(bytes.len(), 8, "tagged frame must carry 8 bytes");
            let input_idx = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
            let seq = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
            (input_idx, seq)
        }
        other => panic!("expected Bgra-tagged frame, got {other:?}"),
    }
}

/// Build a tagged audio batch. Encodes `(input_idx, seq)` into the first
/// sample of a Mono buffer; remaining samples are zero. `len` controls
/// `end_pts` as `start_pts + len`.
pub(crate) fn tagged_audio_batch(
    input_idx: u32,
    seq: u32,
    start_pts: Duration,
    len: Duration,
    sample_rate: u32,
) -> InputAudioSamples {
    let n_samples = ((sample_rate as u64) * len.as_nanos() as u64 / 1_000_000_000) as usize;
    let n_samples = n_samples.max(1);
    let tag_bits = pack_tag(input_idx, seq);
    let tag_f = f64::from_bits(tag_bits);
    let mut samples = vec![0.0_f64; n_samples];
    samples[0] = tag_f;
    InputAudioSamples::new(AudioSamples::Mono(samples), start_pts, sample_rate)
}

/// Read `(input_idx, seq)` from a tagged audio batch.
pub(crate) fn read_audio_tag(batch: &InputAudioSamples) -> (u32, u32) {
    match &batch.samples {
        AudioSamples::Mono(s) => unpack_tag(f64::to_bits(s[0])),
        AudioSamples::Stereo(s) => unpack_tag(f64::to_bits(s[0].0)),
    }
}

// -- Recorder ---------------------------------------------------------------

#[derive(Debug, Default)]
pub(crate) struct Recorded {
    pub video: Vec<QueueVideoOutput>,
    pub audio: Vec<QueueAudioOutput>,
    pub events_fired: Vec<u64>,
}

impl Recorded {
    /// Tags from input N for video output buffer at index `out_idx`.
    pub fn video_tags_for_input(&self, out_idx: usize, input_id: &InputId) -> Vec<(u32, u32)> {
        let buffer = &self.video[out_idx];
        match buffer.frames.get(input_id) {
            Some(PipelineEvent::Data(frame)) => vec![read_video_tag(frame)],
            _ => Vec::new(),
        }
    }

    pub fn video_input_event(
        &self,
        out_idx: usize,
        input_id: &InputId,
    ) -> Option<&PipelineEvent<Frame>> {
        self.video[out_idx].frames.get(input_id)
    }

    pub fn audio_tags_for_input(&self, out_idx: usize, input_id: &InputId) -> Vec<(u32, u32)> {
        match self.audio[out_idx].samples.get(input_id) {
            Some(PipelineEvent::Data(batches)) => batches.iter().map(read_audio_tag).collect(),
            _ => Vec::new(),
        }
    }

    pub fn audio_input_event(
        &self,
        out_idx: usize,
        input_id: &InputId,
    ) -> Option<&PipelineEvent<Vec<InputAudioSamples>>> {
        self.audio[out_idx].samples.get(input_id)
    }
}

// -- Harness ----------------------------------------------------------------

pub(crate) struct QueueHarnessBuilder {
    output_framerate: Framerate,
    ahead_of_time_processing: bool,
    run_late_scheduled_events: bool,
    never_drop_output_frames: bool,
    side_channel_delay: Duration,
}

impl Default for QueueHarnessBuilder {
    fn default() -> Self {
        Self {
            output_framerate: Framerate { num: 30, den: 1 },
            ahead_of_time_processing: true,
            run_late_scheduled_events: false,
            never_drop_output_frames: false,
            side_channel_delay: Duration::ZERO,
        }
    }
}

impl QueueHarnessBuilder {
    pub fn output_framerate(mut self, fps: Framerate) -> Self {
        self.output_framerate = fps;
        self
    }
    pub fn ahead_of_time_processing(mut self, v: bool) -> Self {
        self.ahead_of_time_processing = v;
        self
    }
    pub fn run_late_scheduled_events(mut self, v: bool) -> Self {
        self.run_late_scheduled_events = v;
        self
    }
    pub fn never_drop_output_frames(mut self, v: bool) -> Self {
        self.never_drop_output_frames = v;
        self
    }
    pub fn side_channel_delay(mut self, v: Duration) -> Self {
        self.side_channel_delay = v;
        self
    }

    pub fn build(self) -> QueueHarness {
        let opts = QueueOptions {
            output_framerate: self.output_framerate,
            ahead_of_time_processing: self.ahead_of_time_processing,
            run_late_scheduled_events: self.run_late_scheduled_events,
            never_drop_output_frames: self.never_drop_output_frames,
            side_channel_delay: self.side_channel_delay,
            side_channel_socket_dir: None,
        };
        let clock = Arc::new(VirtualClock::new());
        let ticker = Arc::new(TestTicker::new());
        let shared_clock: SharedClock = clock.clone();
        let shared_ticker: SharedTicker = ticker.clone();
        let queue = Queue::new_inner(opts, shared_clock, shared_ticker);
        let event_emitter = Arc::new(EventEmitter::new());

        QueueHarness {
            queue,
            clock,
            ticker,
            event_emitter,
            output_framerate: self.output_framerate,
            recorder: Arc::new(Mutex::new(Recorded::default())),
            video_recv: Mutex::new(None),
            audio_recv: Mutex::new(None),
            consumer_threads: Mutex::new(Vec::new()),
            input_counter: Mutex::new(0),
        }
    }
}

pub(crate) struct QueueHarness {
    pub queue: Arc<Queue>,
    pub clock: Arc<VirtualClock>,
    pub ticker: Arc<TestTicker>,
    pub event_emitter: Arc<EventEmitter>,
    pub output_framerate: Framerate,
    pub recorder: Arc<Mutex<Recorded>>,
    video_recv: Mutex<Option<Receiver<QueueVideoOutput>>>,
    audio_recv: Mutex<Option<Receiver<QueueAudioOutput>>>,
    consumer_threads: Mutex<Vec<thread::JoinHandle<()>>>,
    input_counter: Mutex<u32>,
}

impl QueueHarness {
    pub fn builder() -> QueueHarnessBuilder {
        QueueHarnessBuilder::default()
    }

    /// Allocate a fresh input idx for tagging (independent of insertion order).
    fn next_idx(&self) -> u32 {
        let mut g = self.input_counter.lock().unwrap();
        let v = *g;
        *g += 1;
        v
    }

    /// Add an input to the queue. Returns an [`InputHandle`] for pushing
    /// frames/samples and managing tracks.
    pub fn add_input(&self, name: &str, opts: QueueInputOptions) -> InputHandle {
        let input_idx = self.next_idx();
        let input_id = InputId(name.into());
        let input_ref = Ref::new(&input_id);
        let queue_input = QueueInput::new_inner(
            self.queue.ctx(),
            self.event_emitter.clone(),
            &input_ref,
            opts.required,
            None,
            None,
        );
        self.queue.add_input(&input_id, queue_input.clone());

        InputHandle {
            input_idx,
            input_id,
            queue_input,
            video_sender: Mutex::new(None),
            audio_sender: Mutex::new(None),
            ticker: self.ticker.clone(),
        }
    }

    /// Start the queue. Spawns consumer threads that drain output channels
    /// into the recorder. Capacity 1024 outputs per side; tests should not
    /// exceed this without periodically asserting.
    pub fn start(&self) {
        let (video_tx, video_rx) = bounded::<QueueVideoOutput>(1024);
        let (audio_tx, audio_rx) = bounded::<QueueAudioOutput>(1024);
        self.queue.start(video_tx, audio_tx);

        let recorder_v = self.recorder.clone();
        let video_consumer = thread::Builder::new()
            .name("test-video-consumer".to_string())
            .spawn(move || {
                while let Ok(out) = video_rx.recv() {
                    recorder_v.lock().unwrap().video.push(out);
                }
            })
            .unwrap();

        let recorder_a = self.recorder.clone();
        let audio_consumer = thread::Builder::new()
            .name("test-audio-consumer".to_string())
            .spawn(move || {
                while let Ok(out) = audio_rx.recv() {
                    recorder_a.lock().unwrap().audio.push(out);
                }
            })
            .unwrap();

        self.consumer_threads
            .lock()
            .unwrap()
            .extend([video_consumer, audio_consumer]);
    }

    /// Start with consumer threads that block briefly before draining (used
    /// by drop-accounting tests).
    pub fn start_with_consumer_lag(&self, lag: Duration) {
        let (video_tx, video_rx) = bounded::<QueueVideoOutput>(1024);
        let (audio_tx, audio_rx) = bounded::<QueueAudioOutput>(1024);
        self.queue.start(video_tx, audio_tx);

        let recorder_v = self.recorder.clone();
        let video_consumer = thread::Builder::new()
            .name("test-video-consumer-slow".to_string())
            .spawn(move || {
                while let Ok(out) = video_rx.recv() {
                    thread::sleep(lag);
                    recorder_v.lock().unwrap().video.push(out);
                }
            })
            .unwrap();

        let recorder_a = self.recorder.clone();
        let audio_consumer = thread::Builder::new()
            .name("test-audio-consumer-slow".to_string())
            .spawn(move || {
                while let Ok(out) = audio_rx.recv() {
                    thread::sleep(lag);
                    recorder_a.lock().unwrap().audio.push(out);
                }
            })
            .unwrap();

        self.consumer_threads
            .lock()
            .unwrap()
            .extend([video_consumer, audio_consumer]);
    }

    /// Advance virtual clock by `d` and pulse the ticker enough times to let
    /// the queue thread settle.
    pub fn advance(&self, d: Duration) {
        self.clock.advance(d);
        self.flush();
    }

    /// Pulse the ticker repeatedly until the queue thread has had a chance to
    /// process. We pulse a fixed number of times and yield, since the test
    /// ticker is bounded but the queue thread may need multiple iterations to
    /// drain per-input state. 32 pulses + small sleeps is empirically enough
    /// for all test scenarios; the cost is bounded so we don't optimize.
    pub fn flush(&self) {
        for _ in 0..32 {
            self.ticker.pulse();
            // Yield so the queue thread has a chance to run between pulses.
            // This is needed because crossbeam_channel::select! uses spin
            // initially before parking, and we want to give it real time to
            // observe the channel.
            thread::sleep(Duration::from_micros(200));
        }
    }

    /// Block until the recorder has at least `n` video buffers, or panic on
    /// timeout. Used after pushing frames + advancing the clock.
    pub fn wait_for_video_count(&self, n: usize) {
        self.wait_until(|r| r.video.len() >= n, "video count");
    }

    pub fn wait_for_audio_count(&self, n: usize) {
        self.wait_until(|r| r.audio.len() >= n, "audio count");
    }

    fn wait_until<F: Fn(&Recorded) -> bool>(&self, pred: F, what: &str) {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            self.flush();
            if pred(&self.recorder.lock().unwrap()) {
                return;
            }
            if Instant::now() > deadline {
                let r = self.recorder.lock().unwrap();
                panic!(
                    "timeout waiting for {what}: video={}, audio={}",
                    r.video.len(),
                    r.audio.len()
                );
            }
            thread::sleep(Duration::from_millis(2));
        }
    }

    /// Snapshot of the recorder state right now.
    pub fn recorded(&self) -> Recorded {
        let r = self.recorder.lock().unwrap();
        Recorded {
            video: r.video.clone(),
            audio: r.audio.clone(),
            events_fired: r.events_fired.clone(),
        }
    }

    /// Output frame interval at the configured framerate.
    pub fn frame_interval(&self) -> Duration {
        self.output_framerate.get_interval_duration()
    }

    /// Output PTS of the i-th video buffer (relative to queue start, plus
    /// queue_start_pts which is whatever the clock said at start time).
    /// Tests usually compare deltas, not absolute values.
    pub fn nominal_video_pts(&self, i: usize) -> Duration {
        Duration::from_secs_f64(
            i as f64 * self.output_framerate.den as f64 / self.output_framerate.num as f64,
        )
    }

    /// Schedule an event at the given (public) PTS. When fired, the tag is
    /// appended to `recorder.events_fired` so tests can assert ordering and
    /// firing.
    pub fn schedule_event(&self, pts: Duration, tag: u64) {
        let recorder = self.recorder.clone();
        self.queue.schedule_event(
            pts,
            Box::new(move || {
                recorder.lock().unwrap().events_fired.push(tag);
            }),
        );
    }
}

impl Drop for QueueHarness {
    fn drop(&mut self) {
        self.queue.shutdown();
        // Pulse so the queue thread observes the close flag.
        self.flush();
        // Consumer threads will exit when their channels disconnect (which
        // happens when the queue thread drops the senders on shutdown).
    }
}

// -- InputHandle ------------------------------------------------------------

pub(crate) struct InputHandle {
    pub input_idx: u32,
    pub input_id: InputId,
    pub queue_input: QueueInput,
    video_sender: Mutex<Option<QueueSender<Frame>>>,
    audio_sender: Mutex<Option<QueueSender<InputAudioSamples>>>,
    /// Used to drain the bounded(1) input channels by pulsing the queue
    /// thread between pushes; otherwise the harness would block on the
    /// second push of any stream.
    ticker: Arc<TestTicker>,
}

impl InputHandle {
    fn drain_pulse(&self) {
        // A few pulses + brief yields gives the queue thread a chance to
        // run try_enqueue and free the bounded(1) input channel. We don't
        // need the full 32-pulse flush here.
        for _ in 0..4 {
            self.ticker.pulse();
            thread::sleep(Duration::from_micros(200));
        }
    }

    /// Queue a track and stash the resulting senders.
    pub fn queue_track(&self, opts: QueueTrackOptions) {
        let (vs, as_) = self.queue_input.queue_new_track(opts);
        if let Some(s) = vs {
            *self.video_sender.lock().unwrap() = Some(s);
        }
        if let Some(s) = as_ {
            *self.audio_sender.lock().unwrap() = Some(s);
        }
    }

    pub fn queue_video_track(&self, offset: QueueTrackOffset) {
        self.queue_track(QueueTrackOptions {
            video: true,
            audio: false,
            offset,
        });
    }

    pub fn queue_audio_track(&self, offset: QueueTrackOffset) {
        self.queue_track(QueueTrackOptions {
            video: false,
            audio: true,
            offset,
        });
    }

    pub fn queue_av_track(&self, offset: QueueTrackOffset) {
        self.queue_track(QueueTrackOptions {
            video: true,
            audio: true,
            offset,
        });
    }

    pub fn abort_old_track(&self) {
        self.queue_input.abort_old_track();
    }

    pub fn pause(&self) {
        self.queue_input.pause();
    }

    pub fn resume(&self) {
        self.queue_input.resume();
    }

    pub fn push_video_frame(&self, seq: u32, input_pts: Duration) {
        let frame = tagged_frame(self.input_idx, seq, input_pts);
        self.video_sender
            .lock()
            .unwrap()
            .as_ref()
            .expect("queue_track with video first")
            .send(frame)
            .expect("queue_input video sender closed");
        self.drain_pulse();
    }

    /// Push frames at PTS `start_pts + n * (1/fps)` for n in [0, count).
    pub fn push_video_stream(&self, fps: Framerate, count: u32, start_pts: Duration) {
        let interval = fps.get_interval_duration();
        for n in 0..count {
            self.push_video_frame(n, start_pts + interval * n);
        }
    }

    pub fn push_video_at(&self, ptss: &[Duration]) {
        for (i, pts) in ptss.iter().enumerate() {
            self.push_video_frame(i as u32, *pts);
        }
    }

    pub fn push_audio_batch(
        &self,
        seq: u32,
        start_pts: Duration,
        len: Duration,
        sample_rate: u32,
    ) {
        let batch = tagged_audio_batch(self.input_idx, seq, start_pts, len, sample_rate);
        self.audio_sender
            .lock()
            .unwrap()
            .as_ref()
            .expect("queue_track with audio first")
            .send(batch)
            .expect("queue_input audio sender closed");
        self.drain_pulse();
    }

    /// Push `count` contiguous audio batches, each `batch_len` long, starting
    /// at `start_pts`. Sample rate is fixed at 48000 to keep durations stable.
    pub fn push_audio_stream(
        &self,
        batch_len: Duration,
        count: u32,
        start_pts: Duration,
    ) {
        for n in 0..count {
            self.push_audio_batch(n, start_pts + batch_len * n, batch_len, 48000);
        }
    }

    pub fn drop_video(&self) {
        self.video_sender.lock().unwrap().take();
    }

    pub fn drop_audio(&self) {
        self.audio_sender.lock().unwrap().take();
    }
}
