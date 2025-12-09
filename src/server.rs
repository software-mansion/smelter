use crossbeam_channel::Receiver;
use signal_hook::{consts, iterator::Signals};
use smelter_render::error::ErrorStack;
use tracing::error;
use tracing::info;

use std::time::Duration;
use std::{net::SocketAddr, process, sync::Arc, thread};
use tokio::runtime::Runtime;

use crate::{config::read_config, logger::init_logger, routes::routes, state::ApiState};

pub fn run() {
    listen_for_parent_termination();
    let config = read_config();
    init_logger(config.logger.clone());

    info!("Starting Smelter with config:\n{:#?}", config);
    let runtime = Arc::new(Runtime::new().unwrap());
    let state = ApiState::new(config, runtime.clone()).unwrap_or_else(|err| {
        panic!(
            "Failed to start Smelter instance.\n{}",
            ErrorStack::new(&err).into_string()
        )
    });
    let chromium_context = state.chromium_context.clone();

    thread::Builder::new()
        .name("HTTP server startup thread".to_string())
        .spawn(move || {
            let (_should_close_sender, should_close_receiver) = crossbeam_channel::bounded(1);
            if let Err(err) = run_api(state, runtime, should_close_receiver) {
                error!(%err);
                process::exit(1);
            }
        })
        .unwrap();
    match chromium_context {
        None => {
            println!("Using signal");
            let mut signals = Signals::new([consts::SIGINT]).unwrap();
            signals.forever().next();
            // println!("Using signal2");
        }
        Some(chromium_context) => {
            if let Err(err) = chromium_context.run_event_loop() {
                panic!(
                    "Failed to start event loop.\n{}",
                    ErrorStack::new(&err).into_string()
                )
            }

            println!("HRER");
        }
    }

    // thread::sleep(Duration::from_secs(15));
    // process::exit(1);
    // thread::sleep(Duration::from_secs(25));
    // unsafe {
    //     libc::exit(1);
    // }
    // panic!();
}

pub fn run_api(
    state: Arc<ApiState>,
    runtime: Arc<Runtime>,
    should_close: Receiver<()>,
) -> tokio::io::Result<()> {
    runtime.block_on(async {
        let port = state.config.api_port;
        let app = routes(state);
        let listener =
            tokio::net::TcpListener::bind(SocketAddr::from(([0, 0, 0, 0], port))).await?;

        axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                should_close.recv().unwrap();
            })
            .await
    })
}

#[cfg(target_os = "linux")]
fn listen_for_parent_termination() {
    use libc::{SIGTERM, prctl};
    unsafe {
        prctl(libc::PR_SET_PDEATHSIG, SIGTERM);
    }
}

#[cfg(target_os = "macos")]
fn listen_for_parent_termination() {
    use libc::SIGTERM;
    use std::{os::unix::process::parent_id, time::Duration};
    let ppid = parent_id();

    thread::Builder::new()
        .name("Parent process pid change".to_string())
        .spawn(move || {
            loop {
                let current_pid = parent_id();
                if current_pid != ppid {
                    info!("Compositor parent process was terminated.");
                    unsafe {
                        libc::kill(std::process::id() as libc::c_int, SIGTERM);
                    }
                }
                thread::sleep(Duration::from_secs(1));
            }
        })
        .unwrap();
}
