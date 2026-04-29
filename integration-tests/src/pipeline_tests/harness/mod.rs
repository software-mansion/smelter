//! Comparison harness for pipeline tests.
//!
//! Replaces the FFT- and frame-index-based comparison logic in
//! `crate::validation` with PTS-aligned comparisons that share their
//! detection primitives with the post-failure inspector. The legacy
//! validators still live alongside this module and will be deleted in
//! a follow-up step.
//!
//! Public API mirrors the old layout: [`compare_video_dumps`] and
//! [`compare_audio_dumps`]. They handle the boilerplate around
//! reading the expected snapshot and dropping a debug bundle into the
//! failed-snapshots directory on mismatch.
//!
//! Set [`SAVE_DUMPS_ENV`] (any non-empty value) to also drop the
//! expected/actual pair on every run, even when the comparison
//! passes — useful when chasing intermittent diffs whose tolerance is
//! still inside the configured threshold.

use std::{fmt, path::Path};

use anyhow::Result;
use bytes::Bytes;

use crate::{output_dump_from_disk, save_failed_actual_dump, save_failed_test_dumps};

pub mod audio;
pub mod audio_analysis;
pub mod fft;
pub mod video;

pub use audio::AudioCompareConfig;
pub use fft::FftCompareConfig;
pub use video::VideoCompareConfig;

/// Env var that, when set to a non-empty value, makes the harness
/// always drop the expected/actual dump pair into the failed-snapshots
/// directory — even on a passing run.
pub const SAVE_DUMPS_ENV: &str = "SMELTER_SAVE_DUMPS";

pub fn compare_video_dumps<P: AsRef<Path> + fmt::Debug>(
    snapshot_filename: P,
    actual: &Bytes,
    config: VideoCompareConfig,
) -> Result<()> {
    let expected = match output_dump_from_disk(&snapshot_filename) {
        Ok(b) => b,
        Err(err) => return handle_missing_expected(err, snapshot_filename, actual),
    };
    let result = video::compare(&expected, actual, config);
    finalize(result, &expected, actual, &snapshot_filename)
}

pub fn compare_audio_dumps<P: AsRef<Path> + fmt::Debug>(
    snapshot_filename: P,
    actual: &Bytes,
    config: AudioCompareConfig,
) -> Result<()> {
    let expected = match output_dump_from_disk(&snapshot_filename) {
        Ok(b) => b,
        Err(err) => return handle_missing_expected(err, snapshot_filename, actual),
    };
    let result = audio::compare(&expected, actual, config);
    finalize(result, &expected, actual, &snapshot_filename)
}

/// Common tail for both compare entry points: on failure, always save
/// both dumps for the inspector. On success, save the pair too if the
/// debug env is set.
fn finalize<P: AsRef<Path> + fmt::Debug>(
    result: Result<()>,
    expected: &Bytes,
    actual: &Bytes,
    snapshot_filename: P,
) -> Result<()> {
    match result {
        Ok(()) => {
            if save_dumps_env_set() {
                save_failed_test_dumps(expected, actual, &snapshot_filename);
            }
            Ok(())
        }
        Err(err) => {
            save_failed_test_dumps(expected, actual, &snapshot_filename);
            Err(err)
        }
    }
}

fn handle_missing_expected<P: AsRef<Path> + fmt::Debug>(
    err: anyhow::Error,
    snapshot_filename: P,
    actual: &Bytes,
) -> Result<()> {
    // Drop the actual dump in the failed-snapshots dir so the
    // inspector has something to look at, even though the expected
    // side doesn't exist (typical for a first run of a new test).
    save_failed_actual_dump(actual, &snapshot_filename);
    Err(err)
}

fn save_dumps_env_set() -> bool {
    std::env::var_os(SAVE_DUMPS_ENV).is_some_and(|v| !v.is_empty())
}
