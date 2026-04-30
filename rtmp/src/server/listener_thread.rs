use std::{
    io::ErrorKind,
    net::{SocketAddr, TcpListener, TcpStream},
    thread,
    time::Duration,
};

use crossbeam_channel::unbounded;
use tracing::{debug, error, info, warn};

use crate::{
    OnConnectionCallback, RtmpConnectionError, RtmpServer, RtmpServerConfig,
    server::{connection_thread::run_connection_thread, instance::ServerConnectionCtx},
    transport::RtmpTransport,
};

pub(super) fn start_listener_thread(
    config: RtmpServerConfig,
    on_connection: OnConnectionCallback,
) -> Result<RtmpServer, std::io::Error> {
    let listener = TcpListener::bind(SocketAddr::from(([0, 0, 0, 0], config.port)))?;
    listener
        .set_nonblocking(true)
        .expect("Cannot set non-blocking TCP input stream");
    info!("RTMP server running on port {}", config.port);

    let (conn_sender, conn_receiver) = unbounded();
    let server = RtmpServer::new(config, conn_sender);

    let on_connection_handle = thread::Builder::new()
        .name("RTMP on_connection processor".to_string())
        .spawn(move || {
            let mut on_connection = on_connection;
            for conn in conn_receiver.into_iter() {
                on_connection(conn)
            }
        })
        .unwrap();

    let server_handle = server.handle();
    let listener_handle = thread::Builder::new()
        .name("RTMP listener thread".to_string())
        .spawn(move || {
            loop {
                if server_handle.should_stop_server() {
                    break;
                }

                match listener.accept() {
                    Ok((socket, peer_addr)) => {
                        debug!("New connection from: {peer_addr:?}");

                        let Some(server) = server_handle.upgrade() else {
                            break;
                        };

                        if let Err(err) = start_connection_thread(&server, socket) {
                            warn!(?err, "Failed to handle incoming connection.")
                        }
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

    server.register_listener_handles(listener_handle, on_connection_handle);

    Ok(server)
}

fn start_connection_thread(
    server: &RtmpServer,
    socket: TcpStream,
) -> Result<(), RtmpConnectionError> {
    let ctx = ServerConnectionCtx::new(server);
    let transport = match server.config().tls {
        Some(tls_config) => RtmpTransport::tls_server_stream(socket, &tls_config)?,
        None => RtmpTransport::tcp_server_stream(socket),
    };

    let thread_handle = thread::Builder::new()
        .name("RTMP connection thread".to_string())
        .spawn(move || {
            if let Err(err) = run_connection_thread(&ctx, transport) {
                error!(?err, "Connection terminated with an error");
            }
        })
        .unwrap();

    server.register_connection_handle(thread_handle);
    Ok(())
}
