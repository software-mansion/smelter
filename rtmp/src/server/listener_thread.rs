use std::{
    io::ErrorKind,
    net::{SocketAddr, TcpListener},
    thread,
    time::Duration,
};

use crossbeam_channel::unbounded;
use tracing::{debug, error, info};

use crate::{
    OnConnectionCallback, RtmpServer, RtmpServerConfig,
    server::{connection_thread::start_server_connection_thread, instance::ServerConnectionCtx},
};

pub(super) fn start_listener_thread(
    config: RtmpServerConfig,
    on_connection: OnConnectionCallback,
) -> Result<RtmpServer, std::io::Error> {
    let port = config.port;
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = TcpListener::bind(addr)?;
    listener.set_nonblocking(true).unwrap();

    let (conn_sender, conn_receiver) = unbounded();
    let server = RtmpServer::new(config, conn_sender);

    info!("RTMP server running on port {port}");
    thread::Builder::new()
        .name("RTMP on_connection processor".to_string())
        .spawn(move || {
            let mut on_connection = on_connection;
            for conn in conn_receiver.into_iter() {
                on_connection(conn)
            }
        })
        .unwrap();

    let server_handle = server.handle();
    thread::Builder::new()
        .name("RTMP listener thread".to_string())
        .spawn(move || {
            loop {
                if server_handle.should_stop_server() {
                    break;
                }

                match listener.accept() {
                    Ok((socket, peer_addr)) => {
                        debug!("New connection from: {peer_addr:?}");

                        let Some(ctx) = ServerConnectionCtx::new(&server_handle) else {
                            break;
                        };
                        let thread_handle = start_server_connection_thread(ctx.clone(), socket);
                        ctx.lock().unwrap().thread_handle = Some(thread_handle)
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
