use std::{fs, path::Path, time::Duration};

use anyhow::Result;
use integration_tests_macros::pipeline_test;
use serde_json::json;
use smelter::config::read_config;

use crate::{
    CompositorInstance, Mp4OutputReceiver,
    media::TestSample,
    pipeline_tests::{
        PipelineTest,
        harness::{
            AudioCompareConfig, FftCompareConfig, VideoCompareConfig, compare_audio_dumps,
            compare_video_dumps,
            fft::{Mode, RealTolerance},
        },
    },
};

#[allow(dead_code)]
pub const TESTS: &[PipelineTest] = &[OFFLINE_PROCESSING];

#[pipeline_test(
    description = "
        Offline (ahead-of-time) processing of an MP4 input into an MP4
        output.

        Rescale a 2-second slice of an MP4 input into a 640x320 H.264 +
        AAC MP4 file and compare the decoded frames/audio against the
        committed snapshot.
    ",
    snapshot_name = "offline_processing_output.mp4"
)]
pub fn offline_processing() -> Result<()> {
    const OUTPUT_FILE: &str = "/tmp/offline_processing_output.mp4";
    if Path::new(OUTPUT_FILE).exists() {
        fs::remove_file(OUTPUT_FILE)?;
    };

    let mut config = read_config();
    config.ahead_of_time_processing = true;
    config.never_drop_output_frames = true;
    let instance = CompositorInstance::start(Some(config));

    let output_receiver =
        Mp4OutputReceiver::start(instance.api_port, "output_1", OUTPUT_FILE.into());

    instance.send_request(
        "input/input_1/register",
        json!({
            "type": "mp4",
            "url": TestSample::BigBuckBunnyH264Opus.url(),
            "offset_ms": 0,
            "required": true
        }),
    )?;

    instance.send_request(
        "output/output_1/register",
        json!({
            "type": "mp4",
            "path": OUTPUT_FILE,
            "video": {
                "resolution": {
                    "width": 640,
                    "height": 320
                },
                "encoder": {
                    "type": "ffmpeg_h264",
                    "preset": "ultrafast",
                },
                "initial": {
                    "root": {
                       "type": "view",
                       "children": [{
                            "type": "rescaler",
                            "child": {
                                "type": "input_stream",
                                "input_id": "input_1"
                            }
                        }]
                    }
                },
                "send_eos_when": { "all_inputs": true }
            },
            "audio": {
                "channels": "stereo",
                "encoder": {
                    "type": "aac",
                    // Pin to the harness analysis sample rate so the
                    // decoded audio lines up with the 48 kHz timeline
                    // the gap/FFT detectors assume.
                    "sample_rate": 48000,
                },
                "initial": {
                    "inputs": [{ "input_id": "input_1" }]
                },
                "send_eos_when": { "all_inputs": true }
            }
        }),
    )?;

    instance.send_request(
        "input/input_1/unregister",
        json!({
            "schedule_time_ms": 2000
        }),
    )?;
    instance.send_request(
        "output/output_1/unregister",
        json!({
            "schedule_time_ms": 2000
        }),
    )?;

    instance.send_request("start", json!({}))?;

    let output_dump = output_receiver.wait_for_output()?;

    compare_video_dumps(
        OUTPUT_DUMP_FILE,
        &output_dump,
        VideoCompareConfig {
            validation_intervals: vec![Duration::ZERO..Duration::from_millis(1800)],
            ..Default::default()
        },
    )?;

    let mut fft_cfg = FftCompareConfig::real(vec![Duration::ZERO..Duration::from_secs(1)]);
    fft_cfg.mode = Mode::Real(RealTolerance {
        max_frequency_level: 5.0,
        average_level: 15.0,
        median_level: 15.0,
        general_level: 5.0,
        ..Default::default()
    });

    compare_audio_dumps(
        OUTPUT_DUMP_FILE,
        &output_dump,
        AudioCompareConfig {
            fft: Some(fft_cfg),
            ..Default::default()
        },
    )?;

    Ok(())
}
