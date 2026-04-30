use std::{path::PathBuf, process::ExitCode};

use anyhow::{Context, Result};
use integration_tests::tools::rtp_player;

fn main() -> ExitCode {
    match run() {
        Ok(status) if status.success() => ExitCode::SUCCESS,
        Ok(_) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("{e:#}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<std::process::ExitStatus> {
    let path: PathBuf = std::env::args_os()
        .nth(1)
        .context("Usage: play_rtp_dump <input_file>")?
        .into();
    rtp_player::play(&path)
}
