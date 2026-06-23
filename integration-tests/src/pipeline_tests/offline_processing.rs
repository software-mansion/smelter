use std::{
    fs,
    path::Path,
    time::{Duration, Instant},
};

use anyhow::{Result, bail};
use bytes::Bytes;
use integration_tests_macros::pipeline_test;
use serde_json::json;
use smelter::config::read_config;
use tokio_tungstenite::tungstenite;

use crate::{
    CompositorInstance,
    media::TestSample,
    pipeline_tests::{
        PipelineTest,
        harness::{
            AudioCompareConfig, FftCompareConfig, VideoCompareConfig,
            compare_audio_dumps, compare_video_dumps,
            fft::{Mode, RealTolerance},
        },
        start_server_msg_listener,
    },
};

#[allow(dead_code)]
pub const TESTS: &[PipelineTest] = &[OFFLINE_PROCESSING];

#[pipeline_test(
    description = "
        Offline (ahead-of-time) processing of an MP4 input into an MP4 output.

        Compose the first 20 seconds of Big Buck Bunny into a 640x320 MP4
        with H264 video and AAC audio. Offline processing is not throttled
        to realtime, so producing the whole file must take less than 10
        seconds.
    ",
    snapshot_name = "offline_processing_output.mp4"
)]
pub fn offline_processing() -> Result<()> {
    const OUTPUT_FILE: &str = "/tmp/offline_processing_output.mp4";
    /// Wall-clock budget for producing the whole output. Generous
    /// enough for slow CI machines, but far below the 20 s a realtime
    /// pipeline would need — so it fails if ahead-of-time processing
    /// stops outpacing realtime.
    const MAX_PROCESSING_TIME: Duration = Duration::from_secs(10);
    if Path::new(OUTPUT_FILE).exists() {
        fs::remove_file(OUTPUT_FILE)?;
    };

    let mut config = read_config();
    config.ahead_of_time_processing = true;
    config.never_drop_output_frames = true;
    let instance = CompositorInstance::start(Some(config));
    let (msg_sender, msg_receiver) = crossbeam_channel::unbounded();
    start_server_msg_listener(instance.api_port, msg_sender);

    instance.send_request(
        "input/input_1/register",
        json!({
            "type": "mp4",
            "path": TestSample::BigBuckBunnyH264AAC.ensure_path()?,
            "offset_ms": 0,
            "decoder_map": {
                "h264": "ffmpeg_h264"
            },
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
                    // The audio analysis runs at 48 kHz; the default
                    // for MP4 outputs is 44.1 kHz.
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
            "schedule_time_ms":  20_000
        }),
    )?;
    instance.send_request(
        "output/output_1/unregister",
        json!({
            "schedule_time_ms": 20_000
        }),
    )?;

    let processing_start = Instant::now();
    instance.send_request("start", json!({}))?;

    for msg in msg_receiver.iter() {
        if let tungstenite::Message::Text(msg) = msg
            && msg.contains("\"type\":\"OUTPUT_DONE\",\"output_id\":\"output_1\"")
        {
            break;
        }
    }

    let processing_time = processing_start.elapsed();
    if processing_time > MAX_PROCESSING_TIME {
        bail!(
            "offline processing took {processing_time:.2?} (allowed {MAX_PROCESSING_TIME:?})"
        );
    }

    let new_output_dump = Bytes::from(fs::read(OUTPUT_FILE)?);

    compare_video_dumps(
        OUTPUT_DUMP_FILE,
        &new_output_dump,
        VideoCompareConfig {
            validation_intervals: vec![Duration::ZERO..Duration::from_millis(18_000)],
            ..Default::default()
        },
    )?;

    let mut fft_cfg =
        FftCompareConfig::real(vec![Duration::ZERO..Duration::from_millis(18_000)]);
    fft_cfg.mode = Mode::Real(RealTolerance {
        max_frequency_level: 5.0,
        average_level: 15.0,
        median_level: 15.0,
        general_level: 5.0,
        ..Default::default()
    });

    compare_audio_dumps(
        OUTPUT_DUMP_FILE,
        &new_output_dump,
        AudioCompareConfig { fft: Some(fft_cfg), ..Default::default() },
    )?;

    Ok(())
}
