#![recursion_limit = "256"]

use tracing::info;

pub mod config;
pub mod error;
pub mod logger;
pub mod middleware;
pub mod routes;
pub mod server;
pub mod state;

fn main() {
    #[cfg(feature = "web-renderer")]
    {
        use libcef::bundle_for_development;

        let target_path = std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .to_owned();
        if bundle_for_development(&target_path).is_err() {
            panic!(
                "Build process helper first. For release profile use: cargo build -r --bin process_helper"
            );
        }
    }

    ffmpeg_next::format::network::init();

    server::run();

    info!("Received exit signal. Terminating...")
    // TODO: add graceful shutdown
}
