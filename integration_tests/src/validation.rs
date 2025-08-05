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

    if let Err(err) = audio::validate(&expected, actual, config) {
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
    // Intervals returned by this function do not match time stamp exactly.
    // They usually are slightly longer, because interval must be split into
    // batches of 16384 samples.
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
        let mut first_sample = (start_pts.as_secs_f64() * sample_rate as f64
            / samples_per_batch as f64) as usize
            * samples_per_batch;

        let mut intervals = vec![];
        loop {
            let pts = start_pts + intervals.len() as u32 * time_per_batch;
            if pts >= end_pts {
                break;
            }

            intervals.push(SamplingInterval {
                first_sample,
                samples: samples_per_batch,
            });
            first_sample += samples_per_batch;
        }
        intervals
    }
}

pub struct AudioAnalyzeTolerance {
    /// Tolerance of max frequency. This value is the multiplier
    /// by which frequency resolution shall be multiplied when comparing values
    pub max_frequency: u32,
    pub max_frequency_level: f32,
    pub average_level: f32,
    pub median_level: f32,
    pub general_level: f64,
}

impl Default for AudioAnalyzeTolerance {
    fn default() -> Self {
        Self {
            // In case of spectral leaking
            max_frequency: 1,
            max_frequency_level: 3.0,
            average_level: 5.0,
            median_level: 5.0,
            general_level: 3.0,
        }
    }
}

pub struct AudioValidationConfig {
    pub sampling_intervals: Vec<Range<Duration>>,
    pub channels: AudioChannels,
    pub sample_rate: u32,
    pub samples_per_batch: usize,
    pub allowed_failed_batches: u8,
    pub tolerance: AudioAnalyzeTolerance,
}

impl Default for AudioValidationConfig {
    fn default() -> Self {
        Self {
            sampling_intervals: vec![Duration::from_secs(0)..Duration::from_secs(1)],
            channels: AudioChannels::Stereo,
            sample_rate: 48000,

            // It HAS TO be a power of 2 for FFT to work.
            // As 'channels' option is always set to stereo this will result in 16384 samples
            // per channel which is approx. 0.34s for the default sample rate.
            // This number MUST NOT exceed 16384 per channel.
            samples_per_batch: 32768,
            allowed_failed_batches: 0,
            tolerance: AudioAnalyzeTolerance::default(),
        }
    }
}
