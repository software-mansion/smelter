use anyhow::Result;
use bytes::Bytes;
use std::{fmt, ops::Range, path::Path, time::Duration};
use tracing::info;

use crate::{
    audio_decoder::AudioChannels, output_dump_from_disk, save_failed_test_dumps,
    update_dump_on_disk,
};

mod audio;
mod video;

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

    let AudioValidationConfig {
        sampling_intervals,
        channels,
        sample_rate,
        samples_per_batch,
        tolerance,
    } = config;

    if let Err(err) = audio::validate(
        &expected,
        actual,
        &sampling_intervals,
        channels,
        sample_rate,
        samples_per_batch,
        tolerance,
    ) {
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

pub struct VideoValidationConfig {
    pub validation_intervals: Vec<Range<Duration>>,
    pub allowed_error: f32,
    pub allowed_invalid_frames: usize,
}

impl Default for VideoValidationConfig {
    fn default() -> Self {
        Self {
            validation_intervals: vec![Duration::from_secs(1)..Duration::from_secs(3)],
            allowed_error: 20.0,
            allowed_invalid_frames: 0,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SamplingInterval {
    pub first_sample: usize,
    pub samples: usize,
}

impl SamplingInterval {
    // Intervals returned are this function are not exact
    // They may be slightly longer as this function fills the time with batches of size
    // specified as argument
    pub fn from_range(
        time_range: &Range<Duration>,
        sample_rate: u32,
        samples_per_batch: usize,
    ) -> Vec<Self> {
        let start_pts = time_range.start;
        let end_pts = time_range.end;
        if end_pts < start_pts {
            return vec![];
        }

        let time_per_batch = Duration::from_secs_f64(samples_per_batch as f64 / sample_rate as f64);

        // It finds the sample that fits pts best
        // If it is not a multiple of samples_per_batch find the highest
        // multiple lower than current number to be the starting sample
        let mut first_sample =
            f64::floor(start_pts.as_secs_f64() * sample_rate as f64 / samples_per_batch as f64)
                as usize
                * samples_per_batch;

        let mut intervals = vec![];
        let mut n = 0;
        loop {
            let pts = start_pts + n * time_per_batch;
            if pts >= end_pts {
                break;
            }

            intervals.push(SamplingInterval {
                first_sample,
                samples: samples_per_batch,
            });
            first_sample += samples_per_batch;
            n += 1;
        }
        intervals
    }
}

// It HAS TO be a power of 2 for FFT to work
// As channels is always set to stereo this will result in 4096 samples
// per channel. This number MUST NOT exceed 16384 per channel
const DEFAULT_SAMPLES_PER_BATCH: usize = 32768;
const DEFAULT_SAMPLE_RATE: u32 = 48000;

// TODO: @jbrs: Remove this annotation before PR
#[allow(dead_code)]
pub struct FFTTolerance {
    /// Tolerance of max frequency. This value is the multiplier
    /// by which frequency resolution shall be multiplied while calculating tolerance
    pub max_frequency: u32,
    pub average_magnitude: f64,
    pub median_magnitude: f64,
    pub avg_level: f64,
}

impl Default for FFTTolerance {
    fn default() -> Self {
        Self {
            max_frequency: 0,
            average_magnitude: 0.005,
            median_magnitude: 0.005,
            avg_level: 3.0,
        }
    }
}

pub struct AudioValidationConfig {
    pub sampling_intervals: Vec<Range<Duration>>,
    pub channels: AudioChannels,
    pub sample_rate: u32,
    pub samples_per_batch: usize,
    pub tolerance: FFTTolerance,
}

impl Default for AudioValidationConfig {
    fn default() -> Self {
        Self {
            sampling_intervals: vec![Duration::from_secs(0)..Duration::from_secs(1)],
            channels: AudioChannels::Stereo,
            sample_rate: DEFAULT_SAMPLE_RATE,
            samples_per_batch: DEFAULT_SAMPLES_PER_BATCH,
            tolerance: FFTTolerance::default(),
        }
    }
}
