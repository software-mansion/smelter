use crossbeam_channel::Receiver;
use signal_hook::{consts, iterator::Signals};
use smelter_render::error::ErrorStack;
use tokio::runtime::Builder;
use tracing::{debug, error, info, trace};

use std::{env, net::SocketAddr, process, sync::Arc, thread};
use tokio::runtime::Runtime;

use crate::{config::read_config, logger::init_logger, routes::routes, state::ApiState};

pub fn run() {
    listen_for_parent_termination();
    let config = read_config();
    init_logger(config.logger.clone());

    info!("Starting Smelter with config:\n{:#?}", config);
    let runtime = Arc::new(init_runtime());
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
            let mut signals = Signals::new([consts::SIGINT]).unwrap();
            signals.forever().next();
        }
        Some(chromium_context) => {
            if let Err(err) = chromium_context.run_event_loop() {
                panic!(
                    "Failed to start event loop.\n{}",
                    ErrorStack::new(&err).into_string()
                )
            }
        }
    }
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

fn init_runtime() -> Runtime {
    const MINIMUM_WORKER_THREADS: usize = 3;

    let available_threads = thread::available_parallelism().ok().map(|v| v.get());
    trace!(available_parallelism=?available_threads, "Available cpus detected.");
    let available_threads = available_threads
        .unwrap_or(MINIMUM_WORKER_THREADS)
        .max(MINIMUM_WORKER_THREADS);

    let thread_count = env::var("TOKIO_WORKER_THREADS")
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(available_threads);

    debug!(
        worker_threads = thread_count,
        "Number of runtime worker threads."
    );
    Builder::new_multi_thread()
        .enable_all()
        .worker_threads(thread_count)
        .build()
        .unwrap()
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
