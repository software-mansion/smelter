use anyhow::Result;
use bytes::Bytes;
use std::{fmt, path::Path};
use tracing::info;

use crate::{
    output_dump_from_disk, save_failed_test_dumps, update_dump_on_disk,
    validation::audio::AudioValidationConfig, video::VideoValidationConfig,
};

pub mod audio;
pub mod video;

pub fn compare_video_dumps<P: AsRef<Path> + fmt::Debug>(
    snapshot_filename: P,
    actual: &Bytes,
    config: VideoValidationConfig,
) -> Result<()> {
    let expected = match output_dump_from_disk(&snapshot_filename) {
        Ok(expected) => expected,
        Err(err) => {
            return handle_error(err, snapshot_filename, actual);
        }
    };

    let VideoValidationConfig {
        validation_intervals,
        allowed_error,
        allowed_invalid_frames,
    } = config;

    if let Err(err) = video::validate(
        &expected,
        actual,
        &validation_intervals,
        allowed_error,
        allowed_invalid_frames,
    ) {
        save_failed_test_dumps(&expected, actual, &snapshot_filename);
        handle_error(err, snapshot_filename, actual)?;
    }

    Ok(())
}

pub fn compare_audio_dumps<P: AsRef<Path> + fmt::Debug>(
    snapshot_filename: P,
    actual: &Bytes,
    config: AudioValidationConfig,
) -> Result<()> {
    let expected = match output_dump_from_disk(&snapshot_filename) {
        Ok(expected) => expected,
        Err(err) => {
            return handle_error(err, snapshot_filename, actual);
        }
    };

    if let Err(err) = audio::validate(&expected, actual, config, audio::ValidationMode::Real) {
        save_failed_test_dumps(&expected, actual, &snapshot_filename);
        handle_error(err, snapshot_filename, actual)?;
    }

    Ok(())
}

fn handle_error<P: AsRef<Path> + fmt::Debug>(
    err: anyhow::Error,
    snapshot_filename: P,
    actual: &Bytes,
) -> Result<()> {
    if cfg!(feature = "update_snapshots") {
        info!("Updating output dump: {snapshot_filename:?}");
        update_dump_on_disk(&snapshot_filename, actual).unwrap();
        return Ok(());
    };

    Err(err)
}
