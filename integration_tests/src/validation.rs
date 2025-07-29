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
        allowed_error,
        channels,
        sample_rate,
    } = config;

    if let Err(err) = audio::validate(
        &expected,
        actual,
        &sampling_intervals,
        allowed_error,
        channels,
        sample_rate,
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

#[derive(Debug)]
pub struct SamplingInterval {
    pub pts: Duration,
    pub samples: usize,
}

impl SamplingInterval {
    pub fn from_range(
        time_range: Range<Duration>,
        sample_rate: u32,
        samples_per_batch: usize,
    ) -> Vec<Self> {
        let start_pts = time_range.start;
        let end_pts = time_range.end;
        if end_pts < start_pts {
            return vec![];
        }

        let time_per_batch = Duration::from_secs_f64(samples_per_batch as f64 / sample_rate as f64);

        let mut intervals = vec![];
        let mut n: u32 = 0;
        loop {
            let next_pts = start_pts + n * time_per_batch;
            if next_pts >= end_pts {
                break;
            }
            let next_interval = SamplingInterval {
                pts: next_pts,
                samples: samples_per_batch,
            };
            intervals.push(next_interval);
            n += 1;
        }
        intervals
    }
}

// TODO: Remove this before PR
#[cfg(test)]
mod interval_calculation_test {
    use std::time::Duration;

    use crate::SamplingInterval;

    #[test]
    fn interval_calc_test() {
        let range = Duration::from_millis(0)..Duration::from_millis(2000);
        let intervals = SamplingInterval::from_range(range, 48000, 4096);
        println!("{:#?}", intervals);
    }
}

pub struct AudioValidationConfig {
    pub sampling_intervals: Vec<SamplingInterval>,
    pub allowed_error: f32,
    pub channels: AudioChannels,
    pub sample_rate: u32,
}

impl Default for AudioValidationConfig {
    fn default() -> Self {
        let default_interval = SamplingInterval {
            pts: Duration::from_millis(0),
            samples: 4096,
        };
        Self {
            sampling_intervals: vec![default_interval],
            allowed_error: 4.0,
            channels: AudioChannels::Stereo,
            sample_rate: 48000,
        }
    }
}
