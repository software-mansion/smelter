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
use smelter_render::{Frame, FramePreProcessor, WgpuCtx};
use tracing::debug;

use crate::prelude::InputAudioSamples;

use super::serialize::{serialize_audio_batch, serialize_rgba_frame};

const ACCEPT_POLL_INTERVAL: Duration = Duration::from_millis(500);
const CLIENT_CHANNEL_CAPACITY: usize = 1;

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
    pub fn new(socket_path: PathBuf, channel_capacity: usize, wgpu_ctx: Arc<WgpuCtx>) -> Self {
        let (clients, cleanup) = bind_and_spawn_accept(socket_path, "video-sc");

        let (sender, receiver) = crossbeam_channel::bounded::<Frame>(channel_capacity);
        thread::Builder::new()
            .name("video-sc-send".to_string())
            .spawn(move || {
                let mut pre_processor = FramePreProcessor::new(wgpu_ctx);
                while let Ok(frame) = receiver.recv() {
                    let resolution = frame.resolution;
                    let pts = frame.pts;
                    let rgba_bytes = pre_processor.process_to_bytes(frame, None);
                    let data = serialize_rgba_frame(resolution, pts, rgba_bytes);
                    send_to_clients(&clients, data);
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
    pub fn new(socket_path: PathBuf, channel_capacity: usize) -> Self {
        let (clients, cleanup) = bind_and_spawn_accept(socket_path, "audio-sc");

        let (sender, receiver) = crossbeam_channel::bounded::<InputAudioSamples>(channel_capacity);
        thread::Builder::new()
            .name("audio-sc-send".to_string())
            .spawn(move || {
                while let Ok(batch) = receiver.recv() {
                    let data = serialize_audio_batch(&batch);
                    send_to_clients(&clients, data);
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
) -> (Clients, Arc<ServerCleanup>) {
    let _ = std::fs::remove_file(&socket_path);
    let listener =
        UnixListener::bind(&socket_path).expect("Failed to bind side channel unix socket");
    listener
        .set_nonblocking(true)
        .expect("Failed to set side channel listener to non-blocking");

    let shutdown = Arc::new(AtomicBool::new(false));
    let clients: Clients = Arc::new(Mutex::new(Vec::new()));
    let clients_accept = clients.clone();
    let shutdown_accept = shutdown.clone();
    thread::Builder::new()
        .name(format!("{name_prefix}-accept"))
        .spawn(move || accept_loop(listener, clients_accept, shutdown_accept, name_prefix))
        .expect("Failed to spawn side channel accept thread");

    let cleanup = Arc::new(ServerCleanup {
        socket_path,
        should_close: shutdown,
    });
    (clients, cleanup)
}

fn accept_loop(
    listener: UnixListener,
    clients: Clients,
    shutdown: Arc<AtomicBool>,
    name_prefix: &'static str,
) {
    while !shutdown.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, _)) => {
                debug!("Side channel: new client connected");
                let (sender, receiver) =
                    crossbeam_channel::bounded::<Bytes>(CLIENT_CHANNEL_CAPACITY);
                thread::Builder::new()
                    .name(format!("{name_prefix}-client"))
                    .spawn(move || client_loop(stream, receiver))
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

fn send_to_clients(clients: &Mutex<Vec<Sender<Bytes>>>, data: Bytes) {
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

fn client_loop(mut stream: UnixStream, receiver: crossbeam_channel::Receiver<Bytes>) {
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
