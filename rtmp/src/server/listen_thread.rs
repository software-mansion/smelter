use std::{
    io::ErrorKind,
    net::{SocketAddr, TcpListener, TcpStream},
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

use tracing::{error, info};

use crate::{
    OnConnectionCallback, RtmpServer, RtmpServerConnectionError, ServerConfig, TlsConfig,
    protocol::byte_stream::RtmpByteStream, server::connection::handle_connection,
    transport::RtmpTransport,
};

pub(super) fn start_listener_thread(
    config: ServerConfig,
    on_connection: OnConnectionCallback,
) -> Result<Arc<Mutex<RtmpServer>>, std::io::Error> {
    let port = config.port;
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = TcpListener::bind(addr)?;
    listener.set_nonblocking(true).unwrap();
    let on_connection = Arc::new(Mutex::new(on_connection));

    let tls = config.tls.clone();
    let shutdown = Arc::new(AtomicBool::new(false));
    let server = Arc::new(Mutex::new(RtmpServer { config, shutdown }));

    let server_weak: Weak<Mutex<RtmpServer>> = Arc::downgrade(&server);

    thread::Builder::new()
        .name("RTMP server".to_string())
        .spawn(move || {
            loop {
                let Some(server) = server_weak.upgrade() else {
                    break;
                };

                if server.lock().unwrap().shutdown.load(Ordering::Relaxed) {
                    break;
                }
                drop(server);

                match listener.accept() {
                    Ok((socket, peer_addr)) => {
                        info!("New connection from: {peer_addr:?}");

                        let on_connection_clone = on_connection.clone();
                        let stream = match create_rtmp_byte_stream(socket, &tls) {
                            Ok(stream) => stream,
                            Err(err) => {
                                error!(?err, "Failed to initialize RTMP connection");
                                continue;
                            }
                        };

                        thread::spawn(move || {
                            if let Err(err) = handle_connection(stream, on_connection_clone) {
                                error!(?err, "Connection terminated with an error");
                            }
                        });
                    }
                    Err(err) if err.kind() == ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(500));
                    }
                    Err(err) => {
                        error!(%err, "Accept error");
                        break;
                    }
                }
            }
        })
        .unwrap();

    Ok(server)
}

fn create_rtmp_byte_stream(
    socket: TcpStream,
    tls: &Option<TlsConfig>,
) -> Result<RtmpByteStream, RtmpServerConnectionError> {
    let should_close = Arc::new(AtomicBool::new(false));
    let transport = match tls {
        Some(tls_config) => RtmpTransport::tls_server_stream(socket, tls_config)?,
        None => RtmpTransport::tcp_server_stream(socket),
    };

    Ok(RtmpByteStream::new(transport, should_close))
}
