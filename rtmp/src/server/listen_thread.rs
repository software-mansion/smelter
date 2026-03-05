use std::{
    io::ErrorKind,
    net::{SocketAddr, TcpListener},
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

use tracing::{error, info};

use crate::{
    OnConnectionCallback, RtmpServer, ServerConfig, server::connection::handle_connection,
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

    let protocol = if config.tls.is_some() {
        "RTMPS"
    } else {
        "RTMP"
    };
    info!("{protocol} server running on port {port}");

    let tls = config.tls.clone();
    let shutdown = Arc::new(AtomicBool::new(false));
    let server = Arc::new(Mutex::new(RtmpServer { config, shutdown }));

    let server_weak: Weak<Mutex<RtmpServer>> = Arc::downgrade(&server);

    thread::Builder::new()
        .name(format!("{protocol} server"))
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
                        let tls = tls.clone();
                        thread::spawn(move || {
                            if let Err(err) = handle_connection(socket, on_connection_clone, tls) {
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
