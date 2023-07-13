mod internal_queue;

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use crossbeam::channel::{tick, unbounded, Receiver, Sender};

use self::internal_queue::{InternalQueue, QueueError};

/// TODO: This should be a rational.
#[derive(Debug, Clone, Copy)]
pub struct Framerate(pub u32);

pub type InputID = u32;

impl Framerate {
    pub fn get_interval_duration(self) -> Duration {
        Duration::from_nanos((1_000_000_000 / self.0).into())
    }
}

pub struct MockFrame {
    y_plane: bytes::Bytes,
    u_plane: bytes::Bytes,
    v_plane: bytes::Bytes,
    pts: PTS,
}

/// nanoseconds
type PTS = u64;

pub struct FramesBatch {
    frames: HashMap<InputID, Arc<MockFrame>>,
    pts: PTS,
}

impl FramesBatch {
    pub fn new(pts: PTS) -> Self {
        FramesBatch {
            frames: HashMap::new(),
            pts,
        }
    }

    pub fn insert_frame(&mut self, input_id: InputID, frame: Arc<MockFrame>) {
        self.frames.insert(input_id, frame);
    }
}

struct CheckQueueChannel {
    sender: Sender<()>,
    receiver: Receiver<()>,
}

impl CheckQueueChannel {
    pub fn new() -> Self {
        let (sender, receiver) = unbounded();
        CheckQueueChannel { sender, receiver }
    }

    pub fn add_queue_check(&self) {
        self.sender.send(()).unwrap();
    }
}

pub struct Queue {
    internal_queue: Arc<Mutex<InternalQueue>>,
    check_queue_events_channel: Arc<Mutex<CheckQueueChannel>>,
    output_framerate: Framerate,
    frames_sent: u32,
    time_buffer_duration: Duration,
}

impl Queue {
    pub fn new(output_framerate: Framerate) -> Self {
        Queue {
            internal_queue: Arc::new(Mutex::new(InternalQueue::new())),
            check_queue_events_channel: Arc::new(Mutex::new(CheckQueueChannel::new())),
            output_framerate,
            frames_sent: 0,
            time_buffer_duration: Duration::from_millis(100),
        }
    }

    pub fn add_input(&self, input_id: InputID) {
        let mut internal_queue = self.internal_queue.lock().unwrap();
        internal_queue.add_input(input_id);
    }

    #[allow(dead_code)]
    pub fn remove_input(&self, input_id: InputID) {
        let mut internal_queue = self.internal_queue.lock().unwrap();
        // TODO: gracefully remove input - wait until last enqueued frame PTS is smaller than output PTS
        internal_queue.remove_input(input_id);
    }

    #[allow(dead_code)]
    pub fn start(mut self, sender: Sender<FramesBatch>) {
        // Starting timer
        let frame_interval_duration = self.output_framerate.get_interval_duration();
        let ticker = tick(frame_interval_duration);

        let check_queue_events_channel = self.check_queue_events_channel.clone();
        thread::spawn(move || loop {
            ticker.recv().unwrap();
            check_queue_events_channel.lock().unwrap().add_queue_check();
        });

        let start = Instant::now();
        // Checking queue
        thread::spawn(move || loop {
            self.check_queue_events_channel
                .lock()
                .unwrap()
                .receiver
                .recv()
                .unwrap();

            let mut internal_queue = self.internal_queue.lock().unwrap();
            let buffer_pts = self.get_next_output_buffer_pts();
            let next_buffer_time = self.output_framerate.get_interval_duration() * self.frames_sent
                + self.time_buffer_duration;

            if start.elapsed() > next_buffer_time
                || internal_queue.check_all_inputs_ready(buffer_pts)
            {
                let frames_batch = internal_queue.get_frames_batch(buffer_pts);
                sender.send(frames_batch).unwrap();
                self.frames_sent += 1;
                internal_queue.drop_useless_frames(self.get_next_output_buffer_pts());
            }
        });
    }

    #[allow(dead_code)]
    pub fn enqueue_frame(&self, input_id: InputID, frame: MockFrame) -> Result<(), QueueError> {
        let mut internal_queue = self.internal_queue.lock().unwrap();

        internal_queue.enqueue_frame(input_id, frame)?;
        internal_queue.drop_pad_useless_frames(input_id, self.get_next_output_buffer_pts())?;

        self.check_queue_events_channel
            .lock()
            .unwrap()
            .add_queue_check();

        Ok(())
    }

    fn get_next_output_buffer_pts(&self) -> PTS {
        let nanoseconds_in_second = 1_000_000_000;
        (nanoseconds_in_second * (self.frames_sent as u64 + 1)) / self.output_framerate.0 as u64
    }
}
