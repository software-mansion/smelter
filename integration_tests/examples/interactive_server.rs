use smelter::server;
use std::{process, thread, time::Duration};
use tracing::error;

use integration_tests::examples::{
    download_all_assets, start_server_msg_listener, wait_for_server_ready,
};

fn main() {
    ffmpeg_next::format::network::init();

    download_all_assets().unwrap();
    thread::spawn(|| {
        if let Err(err) = wait_for_server_ready(Duration::from_secs(10)) {
            error!("{err}");
            process::exit(1);
        }

        start_server_msg_listener();
    });
    server::run();
}
