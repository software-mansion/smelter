use std::{
    io::Write,
    os::unix::net::{UnixListener, UnixStream},
    path::PathBuf,
    sync::{Arc, Mutex},
    thread,
};

use crossbeam_channel::Sender;
use tracing::debug;

struct SocketCleanup {
    socket_path: PathBuf,
}

impl Drop for SocketCleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.socket_path);
    }
}

#[derive(Clone)]
pub(super) struct SideChannelServer {
    pub sender: Sender<Vec<u8>>,
    _cleanup: Arc<SocketCleanup>,
}

impl SideChannelServer {
    pub fn new(socket_path: PathBuf, name_prefix: &'static str, channel_capacity: usize) -> Self {
        let _ = std::fs::remove_file(&socket_path);
        let listener =
            UnixListener::bind(&socket_path).expect("Failed to bind side channel unix socket");

        let clients: Arc<Mutex<Vec<UnixStream>>> = Arc::new(Mutex::new(Vec::new()));
        let clients_accept = clients.clone();
        thread::Builder::new()
            .name(format!("{name_prefix}-accept"))
            .spawn(move || accept_loop(listener, clients_accept))
            .expect("Failed to spawn side channel accept thread");

        let (sender, receiver) = crossbeam_channel::bounded::<Vec<u8>>(channel_capacity);
        thread::Builder::new()
            .name(format!("{name_prefix}-send"))
            .spawn(move || {
                while let Ok(data) = receiver.recv() {
                    send_to_clients(&clients, &data);
                }
                debug!("{name_prefix} sender thread finished");
            })
            .expect("Failed to spawn side channel send thread");

        Self {
            sender,
            _cleanup: Arc::new(SocketCleanup { socket_path }),
        }
    }
}

fn accept_loop(listener: UnixListener, clients: Arc<Mutex<Vec<UnixStream>>>) {
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                debug!("Side channel: new client connected");
                clients.lock().unwrap().push(stream);
            }
            Err(e) => {
                debug!("Side channel: accept error: {e}");
                break;
            }
        }
    }
}

fn send_to_clients(clients: &Mutex<Vec<UnixStream>>, data: &[u8]) {
    let mut clients = clients.lock().unwrap();
    clients.retain_mut(|stream| {
        let len_bytes = (data.len() as u32).to_be_bytes();
        if stream.write_all(&len_bytes).is_err() {
            debug!("Side channel: client disconnected (length write)");
            return false;
        }
        if stream.write_all(data).is_err() {
            debug!("Side channel: client disconnected (data write)");
            return false;
        }
        true
    });
}
