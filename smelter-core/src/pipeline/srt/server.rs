use std::{
    collections::HashMap,
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
};

use crossbeam_channel::{Receiver, Sender, TrySendError, bounded};
use libsrt::{EPOLL_ERR, EPOLL_IN, EpollEvent, ListenCallbackHandle, SrtEpoll, SrtSocket};
use smelter_render::error::ErrorStack;
use tracing::{Level, debug, error, info, span, warn};

use crate::pipeline::srt::srt_input::{
    connection::start_connection_thread,
    state::{SrtInputState, SrtInputStateOptions},
};

use crate::prelude::*;

const ACCEPT_POLL_TIMEOUT_MS: i64 = 500;

pub struct SrtPipelineState {
    pub port: u16,
    pub inputs: SrtInputsState,
    pub outputs: SrtOutputsState,
}

impl SrtPipelineState {
    pub fn new(port: u16) -> Arc<Self> {
        Arc::new(Self {
            port,
            inputs: SrtInputsState::default(),
            outputs: SrtOutputsState::default(),
        })
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct SrtInputsState(Arc<Mutex<HashMap<Ref<InputId>, SrtInputState>>>);

impl SrtInputsState {
    pub fn get_mut_with<T, Func: FnOnce(&mut SrtInputState) -> Result<T, SrtServerError>>(
        &self,
        input_ref: &Ref<InputId>,
        func: Func,
    ) -> Result<T, SrtServerError> {
        let mut guard = self.0.lock().unwrap();
        match guard.get_mut(input_ref) {
            Some(input) => func(input),
            None => Err(SrtServerError::InputNotFound(input_ref.id().clone())),
        }
    }

    pub(crate) fn add_input(
        &self,
        input_ref: &Ref<InputId>,
        options: SrtInputStateOptions,
    ) -> Result<(), SrtServerError> {
        let mut guard = self.0.lock().unwrap();
        if guard.contains_key(input_ref) {
            return Err(SrtServerError::InputAlreadyRegistered(
                input_ref.id().clone(),
            ));
        }
        if guard.values().any(|i| i.stream_id == options.stream_id) {
            return Err(SrtServerError::StreamIdAlreadyUsed(options.stream_id));
        }
        guard.insert(input_ref.clone(), SrtInputState::new(options));
        Ok(())
    }

    pub(crate) fn remove_input(&self, input_ref: &Ref<InputId>) {
        let mut guard = self.0.lock().unwrap();
        if guard.remove(input_ref).is_none() {
            error!(?input_ref, "Failed to remove SRT input, ID not found");
        }
    }

    pub(crate) fn find_by_stream_id(
        &self,
        stream_id: &str,
    ) -> Result<Ref<InputId>, SrtServerError> {
        let guard = self.0.lock().unwrap();
        let (input_ref, _) = guard
            .iter()
            .find(|(_, input)| input.stream_id.as_ref() == stream_id)
            .ok_or_else(|| SrtServerError::NotRegisteredStreamId(Arc::from(stream_id)))?;
        Ok(input_ref.clone())
    }

    pub(crate) fn contains_stream_id(&self, stream_id: &str) -> bool {
        let guard = self.0.lock().unwrap();
        guard.values().any(|i| i.stream_id.as_ref() == stream_id)
    }

    pub(crate) fn encryption_for_stream_id(&self, stream_id: &str) -> Option<SrtInputEncryption> {
        let guard = self.0.lock().unwrap();
        guard
            .values()
            .find(|i| i.stream_id.as_ref() == stream_id)
            .and_then(|i| i.encryption.clone())
    }
}

#[derive(Debug)]
struct SrtOutputEntry {
    stream_id: Arc<str>,
    encryption: Option<SrtOutputEncryption>,
    socket_sender: Sender<SrtSocket>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct SrtOutputsState(Arc<Mutex<HashMap<Ref<OutputId>, SrtOutputEntry>>>);

impl SrtOutputsState {
    /// Register an output and return a receiver that the server will use to
    /// hand off accepted SRT sockets matching `stream_id`. The channel is
    /// bounded to 1 — if a caller arrives while the previous one is still
    /// being served, it is refused at dispatch time.
    pub(crate) fn add_output(
        &self,
        output_ref: &Ref<OutputId>,
        stream_id: Arc<str>,
        encryption: Option<SrtOutputEncryption>,
        inputs: &SrtInputsState,
    ) -> Result<Receiver<SrtSocket>, SrtServerError> {
        let mut guard = self.0.lock().unwrap();
        if guard.contains_key(output_ref) {
            return Err(SrtServerError::OutputAlreadyRegistered(
                output_ref.id().clone(),
            ));
        }
        if guard.values().any(|o| o.stream_id == stream_id) || inputs.contains_stream_id(&stream_id)
        {
            return Err(SrtServerError::StreamIdAlreadyUsed(stream_id));
        }
        let (sender, receiver) = bounded(1);
        guard.insert(
            output_ref.clone(),
            SrtOutputEntry {
                stream_id,
                encryption,
                socket_sender: sender,
            },
        );
        Ok(receiver)
    }

    pub(crate) fn encryption_for_stream_id(&self, stream_id: &str) -> Option<SrtOutputEncryption> {
        let guard = self.0.lock().unwrap();
        guard
            .values()
            .find(|o| o.stream_id.as_ref() == stream_id)
            .and_then(|o| o.encryption.clone())
    }

    pub(crate) fn remove_output(&self, output_ref: &Ref<OutputId>) {
        let mut guard = self.0.lock().unwrap();
        if guard.remove(output_ref).is_none() {
            error!(?output_ref, "Failed to remove SRT output, ID not found");
        }
    }

    pub(crate) fn contains_stream_id(&self, stream_id: &str) -> bool {
        let guard = self.0.lock().unwrap();
        guard.values().any(|o| o.stream_id.as_ref() == stream_id)
    }

    /// Try to hand off an accepted socket to the output registered under
    /// `stream_id`. Returns `Ok(())` on success, `Err(sock)` if the output is
    /// not registered or is already serving another caller (so the socket
    /// should be dropped).
    fn try_dispatch(&self, stream_id: &str, sock: SrtSocket) -> Result<(), SrtSocket> {
        let guard = self.0.lock().unwrap();
        let Some(entry) = guard.values().find(|o| o.stream_id.as_ref() == stream_id) else {
            return Err(sock);
        };
        match entry.socket_sender.try_send(sock) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(s)) | Err(TrySendError::Disconnected(s)) => Err(s),
        }
    }
}

pub struct SrtServer {
    shutdown: Arc<AtomicBool>,
    accept_handle: Option<JoinHandle<()>>,
    // Kept alive to own the listen-callback closure for the lifetime of the listener.
    _listen_cb: Option<ListenCallbackHandle>,
}

impl Drop for SrtServer {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        if let Some(handle) = self.accept_handle.take() {
            let _ = handle.join();
        }
    }
}

pub fn spawn_srt_server(
    ctx: Arc<PipelineCtx>,
    state: &SrtPipelineState,
) -> Result<SrtServer, InitPipelineError> {
    let port = state.port;
    let inputs = state.inputs.clone();
    let outputs = state.outputs.clone();

    let listener = SrtSocket::new().map_err(InitPipelineError::SrtServerInitError)?;
    listener
        .set_nonblocking(true)
        .map_err(InitPipelineError::SrtServerInitError)?;
    let addr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, port));
    listener
        .bind(addr)
        .map_err(InitPipelineError::SrtServerInitError)?;

    let inputs_for_cb = inputs.clone();
    let outputs_for_cb = outputs.clone();
    let listen_cb = listener
        .set_listen_callback(move |stream_id, pending| {
            if inputs_for_cb.contains_stream_id(stream_id) {
                if let Some(encryption) = inputs_for_cb.encryption_for_stream_id(stream_id) {
                    if let Err(err) = pending.set_pbkeylen(encryption.key_length.pbkeylen()) {
                        warn!(stream_id, "Failed to set SRT pbkeylen: {err}");
                        return Err(());
                    }
                    if let Err(err) = pending.set_passphrase(&encryption.passphrase) {
                        warn!(stream_id, "Failed to set SRT passphrase: {err}");
                        return Err(());
                    }
                }
                Ok(())
            } else if outputs_for_cb.contains_stream_id(stream_id) {
                if let Some(encryption) = outputs_for_cb.encryption_for_stream_id(stream_id) {
                    if let Err(err) = pending.set_pbkeylen(encryption.key_length.pbkeylen()) {
                        warn!(stream_id, "Failed to set SRT pbkeylen: {err}");
                        return Err(());
                    }
                    if let Err(err) = pending.set_passphrase(&encryption.passphrase) {
                        warn!(stream_id, "Failed to set SRT passphrase: {err}");
                        return Err(());
                    }
                }
                Ok(())
            } else {
                warn!(stream_id, "Rejecting SRT connection: unknown streamid");
                Err(())
            }
        })
        .map_err(InitPipelineError::SrtServerInitError)?;

    listener
        .listen(64)
        .map_err(InitPipelineError::SrtServerInitError)?;

    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_clone = shutdown.clone();
    let accept_handle = thread::Builder::new()
        .name(format!("SRT server :{port}"))
        .spawn(move || {
            let _span = span!(Level::INFO, "SRT server", port = port).entered();
            info!("SRT server listening");
            run_accept_loop(ctx, inputs, outputs, listener, shutdown_clone);
        })
        .unwrap();

    Ok(SrtServer {
        shutdown,
        accept_handle: Some(accept_handle),
        _listen_cb: Some(listen_cb),
    })
}

fn run_accept_loop(
    ctx: Arc<PipelineCtx>,
    inputs: SrtInputsState,
    outputs: SrtOutputsState,
    listener: SrtSocket,
    shutdown: Arc<AtomicBool>,
) {
    let epoll = match SrtEpoll::new() {
        Ok(e) => e,
        Err(err) => {
            error!("Failed to create SRT epoll for listener: {err}");
            return;
        }
    };
    if let Err(err) = epoll.add(&listener, EPOLL_IN | EPOLL_ERR) {
        error!("Failed to add SRT listener to epoll: {err}");
        return;
    }
    let mut events = [EpollEvent { sock: 0, events: 0 }; 1];
    loop {
        if shutdown.load(Ordering::Relaxed) {
            break;
        }
        match epoll.wait(&mut events, ACCEPT_POLL_TIMEOUT_MS) {
            Ok(0) => continue,
            Ok(_) => match listener.accept() {
                Ok((sock, _addr)) => {
                    if let Err(err) =
                        handle_incoming_connection(ctx.clone(), &inputs, &outputs, sock)
                    {
                        warn!(
                            "Failed to handle incoming SRT connection: {}",
                            ErrorStack::new(&err).into_string()
                        );
                    }
                }
                Err(err) => {
                    warn!("SRT accept failed: {err}");
                }
            },
            Err(err) => {
                debug!("SRT epoll wait on listener failed: {err}");
            }
        }
    }
    info!("SRT server stopped");
}

fn handle_incoming_connection(
    ctx: Arc<PipelineCtx>,
    inputs: &SrtInputsState,
    outputs: &SrtOutputsState,
    sock: SrtSocket,
) -> Result<(), SrtServerError> {
    sock.set_nonblocking(true)
        .map_err(|_| SrtServerError::StreamIdNotUtf8)?;
    let stream_id = sock
        .stream_id()
        .map_err(|_| SrtServerError::StreamIdNotUtf8)?;

    if inputs.contains_stream_id(&stream_id) {
        let input_ref = inputs.find_by_stream_id(&stream_id)?;
        return inputs.get_mut_with(&input_ref, |input| {
            input.ensure_no_active_connection(&input_ref)?;
            let handle = start_connection_thread(ctx, &input_ref, input, sock);
            input.connection_handle = handle;
            Ok(())
        });
    }

    match outputs.try_dispatch(&stream_id, sock) {
        Ok(()) => Ok(()),
        Err(_sock) => {
            warn!(
                stream_id,
                "Refusing SRT caller: output is already serving another caller or was unregistered"
            );
            Ok(())
        }
    }
}
