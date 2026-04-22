use std::{
    io::{self, Write},
    os::unix::net::{UnixListener, UnixStream},
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

use bytes::Bytes;
use crossbeam_channel::{Sender, TrySendError};
use smelter_render::{Frame, FramePreProcessor, InputId, WgpuCtx};
use tracing::{Span, debug, info_span};

use crate::prelude::InputAudioSamples;

use super::serialize::{serialize_audio_batch, serialize_rgba_frame};

const ACCEPT_POLL_INTERVAL: Duration = Duration::from_millis(500);
const VIDEO_CHANNEL_CAPACITY: usize = 1;
const AUDIO_CHANNEL_CAPACITY: usize = 10;

struct ServerCleanup {
    socket_path: PathBuf,
    should_close: Arc<AtomicBool>,
}

impl Drop for ServerCleanup {
    fn drop(&mut self) {
        self.should_close.store(true, Ordering::Relaxed);
        let _ = std::fs::remove_file(&self.socket_path);
    }
}

#[derive(Clone)]
pub(super) struct VideoSideChannelServer {
    pub sender: Sender<Frame>,
    _cleanup: Arc<ServerCleanup>,
}

impl VideoSideChannelServer {
    pub fn new(socket_path: PathBuf, input_id: &InputId, wgpu_ctx: Arc<WgpuCtx>) -> Self {
        let span = info_span!("side_channel", kind = "video", input_id = %input_id);
        let (clients, cleanup) = bind_and_spawn_accept(
            socket_path,
            "video-sc",
            VIDEO_CHANNEL_CAPACITY,
            span.clone(),
        );

        let (sender, receiver) = crossbeam_channel::bounded::<Frame>(VIDEO_CHANNEL_CAPACITY);
        thread::Builder::new()
            .name("video-sc-send".to_string())
            .spawn(move || {
                let _enter = span.entered();
                let mut pre_processor = FramePreProcessor::new(wgpu_ctx);
                while let Ok(frame) = receiver.recv() {
                    let resolution = frame.resolution;
                    let pts = frame.pts;
                    let rgba_bytes = pre_processor.process_to_bytes(frame, None);
                    let data = serialize_rgba_frame(resolution, pts, rgba_bytes);
                    broadcast_to_client_threads(&clients, data);
                }
                debug!("video-sc-send thread finished");
            })
            .expect("Failed to spawn video side channel send thread");

        Self {
            sender,
            _cleanup: cleanup,
        }
    }
}

#[derive(Clone)]
pub(super) struct AudioSideChannelServer {
    pub sender: Sender<InputAudioSamples>,
    _cleanup: Arc<ServerCleanup>,
}

impl AudioSideChannelServer {
    pub fn new(socket_path: PathBuf, input_id: &InputId) -> Self {
        let span = info_span!("side_channel", kind = "audio", input_id = %input_id);
        let (clients, cleanup) = bind_and_spawn_accept(
            socket_path,
            "audio-sc",
            AUDIO_CHANNEL_CAPACITY,
            span.clone(),
        );

        let (sender, receiver) =
            crossbeam_channel::bounded::<InputAudioSamples>(AUDIO_CHANNEL_CAPACITY);
        thread::Builder::new()
            .name("audio-sc-send".to_string())
            .spawn(move || {
                let _enter = span.entered();
                while let Ok(batch) = receiver.recv() {
                    let data = serialize_audio_batch(&batch);
                    broadcast_to_client_threads(&clients, data);
                }
                debug!("audio-sc-send thread finished");
            })
            .expect("Failed to spawn audio side channel send thread");

        Self {
            sender,
            _cleanup: cleanup,
        }
    }
}

type Clients = Arc<Mutex<Vec<Sender<Bytes>>>>;

fn bind_and_spawn_accept(
    socket_path: PathBuf,
    name_prefix: &'static str,
    client_channel_capacity: usize,
    span: Span,
) -> (Clients, Arc<ServerCleanup>) {
    let _ = std::fs::remove_file(&socket_path);
    let listener =
        UnixListener::bind(&socket_path).expect("Failed to bind side channel unix socket");
    listener
        .set_nonblocking(true)
        .expect("Failed to set side channel listener to non-blocking");

    let should_close = Arc::new(AtomicBool::new(false));
    let clients: Clients = Arc::new(Mutex::new(Vec::new()));
    let clients_accept = clients.clone();
    let should_close_clone = should_close.clone();
    thread::Builder::new()
        .name(format!("{name_prefix}-accept"))
        .spawn(move || {
            let _enter = span.entered();
            run_accept_clients_thread(
                listener,
                clients_accept,
                should_close_clone,
                name_prefix,
                client_channel_capacity,
            )
        })
        .expect("Failed to spawn side channel accept thread");

    let cleanup = Arc::new(ServerCleanup {
        socket_path,
        should_close,
    });
    (clients, cleanup)
}

fn run_accept_clients_thread(
    listener: UnixListener,
    clients: Clients,
    should_close: Arc<AtomicBool>,
    name_prefix: &'static str,
    client_channel_capacity: usize,
) {
    while !should_close.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, _)) => {
                debug!("Side channel: new client connected");
                let (sender, receiver) =
                    crossbeam_channel::bounded::<Bytes>(client_channel_capacity);
                let client_span = Span::current();
                thread::Builder::new()
                    .name(format!("{name_prefix}-client"))
                    .spawn(move || {
                        let _enter = client_span.entered();
                        run_client_sender_thread(stream, receiver);
                    })
                    .expect("Failed to spawn side channel client thread");
                clients.lock().unwrap().push(sender);
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(ACCEPT_POLL_INTERVAL);
            }
            Err(e) => {
                debug!("Side channel: accept error: {e}");
                break;
            }
        }
    }
    debug!("Side channel: accept thread finished");
}

fn run_client_sender_thread(mut stream: UnixStream, receiver: crossbeam_channel::Receiver<Bytes>) {
    while let Ok(data) = receiver.recv() {
        let len_bytes = (data.len() as u32).to_be_bytes();
        if stream.write_all(&len_bytes).is_err() {
            debug!("Side channel: client write failed (length)");
            return;
        }
        if stream.write_all(&data).is_err() {
            debug!("Side channel: client write failed (data)");
            return;
        }
    }
}

fn broadcast_to_client_threads(clients: &Mutex<Vec<Sender<Bytes>>>, data: Bytes) {
    let mut clients = clients.lock().unwrap();
    clients.retain(|sender| match sender.try_send(data.clone()) {
        Ok(()) => true,
        Err(TrySendError::Full(_)) => {
            debug!("Side channel: dropping message, client channel full");
            true
        }
        Err(TrySendError::Disconnected(_)) => {
            debug!("Side channel: client disconnected");
            false
        }
    });
}
