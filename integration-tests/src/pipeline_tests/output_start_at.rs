//! Realtime (not ahead-of-time) coverage for the MP4 output `start_at_ms` option.
//!
//! Runs with the default config, so the queue is throttled to realtime
//! and the scheduled start has to happen on the wall clock. The MP4
//! muxer normalizes the first chunk to PTS 0, so an output that starts
//! at 4 s and is unregistered at 12 s produces an 8 s file whose PTS 0
//! is the input at 4 s — the duration checks cover the former, the
//! snapshot the latter.
//!
//! The second test is the baseline it is measured against: the same 4 s
//! delay, but produced by sleeping before the register request instead
//! of scheduling the start.

use std::{fs, path::Path, thread, time::Duration};

use anyhow::{Result, bail};
use bytes::Bytes;
use crossbeam_channel::Receiver;
use integration_tests_macros::pipeline_test;
use serde_json::json;
use smelter_render::Frame;
use tokio_tungstenite::tungstenite;

use crate::{
    CompositorInstance,
    media::TestSample,
    pipeline_tests::{
        PipelineTest,
        harness::{
            AudioCompareConfig, FftCompareConfig, VideoCompareConfig, compare_audio_dumps,
            compare_video_dumps,
            fft::{Mode, RealTolerance},
        },
        start_server_msg_listener,
    },
    tools::{
        mp4_source::{Mp4VideoFrameSource, decode_aac_audio},
        video_diff_iter::LazyFrameSource,
    },
};

#[allow(dead_code)]
pub const TESTS: &[PipelineTest] = &[MP4_OUTPUT_START_AT, MP4_OUTPUT_START_AT_DEFAULT];

#[pipeline_test(
    description = "
        Realtime MP4 output registered with `start_at_ms`.

        Start the queue, then register an MP4 output with `start_at_ms: 4000` and
        unregister it at 12 s. The file is created while the register request is
        handled, but the output is only connected to the renderer/audio mixer once the
        queue reaches 4 s, so it holds 8 s of video and audio instead of 12 s.
    ",
    snapshot_name = "mp4_output_start_at.mp4"
)]
pub fn mp4_output_start_at() -> Result<()> {
    const START_AT: Duration = Duration::from_millis(4000);
    const UNREGISTER_AT: Duration = Duration::from_secs(12);

    let output_file = format!("/tmp/{OUTPUT_DUMP_FILE}");
    if Path::new(&output_file).exists() {
        fs::remove_file(&output_file)?;
    }

    let instance = CompositorInstance::start(None);
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

    instance.send_request("start", json!({}))?;

    register_mp4_output(&instance, &output_file, Some(START_AT))?;

    // The output is created while the register request is handled, well before the
    // queue reaches `start_at` — only the moment it starts receiving media is delayed.
    if !Path::new(&output_file).exists() {
        bail!("{output_file} was not created when the output was registered");
    }

    instance.send_request(
        "output/output_1/unregister",
        json!({
            "schedule_time_ms": UNREGISTER_AT.as_millis()
        }),
    )?;

    wait_for_output_done(&msg_receiver, "output_1");

    let dump = Bytes::from(fs::read(&output_file)?);
    let expected = UNREGISTER_AT - START_AT;

    check_duration("video", video_duration(&dump)?, expected)?;
    check_duration("audio", audio_duration(&dump)?, expected)?;

    // The durations above only prove the output ran for the right amount of time.
    // The snapshot proves it started at the right point in the input: PTS 0 of the
    // file has to be `input_1` at 4 s, not at 0 s.
    compare_video_dumps(
        OUTPUT_DUMP_FILE,
        &dump,
        VideoCompareConfig {
            validation_intervals: vec![Duration::ZERO..Duration::from_secs(7)],
            max_failed_pairs: 10,
            ..Default::default()
        },
    )?;

    let mut fft_cfg = FftCompareConfig::real(vec![Duration::ZERO..Duration::from_secs(7)]);
    fft_cfg.mode = Mode::Real(RealTolerance {
        max_frequency_level: 5.0,
        average_level: 15.0,
        median_level: 15.0,
        general_level: 5.0,
        ..Default::default()
    });

    compare_audio_dumps(
        OUTPUT_DUMP_FILE,
        &dump,
        AudioCompareConfig {
            fft: Some(fft_cfg),
            ..Default::default()
        },
    )?;

    Ok(())
}

#[pipeline_test(
    description = "
        Realtime MP4 output registered without `start_at_ms`, as a baseline for
        `mp4_output_start_at`.

        Start the queue, sleep 4 s, then register an MP4 output and unregister it at
        12 s. The output starts as soon as the register request is handled, so the same
        8 s of media ends up in the file — reached by delaying the request instead of
        scheduling the start.
    ",
    snapshot_name = "mp4_output_start_at_default.mp4"
)]
pub fn mp4_output_start_at_default() -> Result<()> {
    const REGISTER_AT: Duration = Duration::from_millis(4000);
    const UNREGISTER_AT: Duration = Duration::from_secs(12);

    let output_file = format!("/tmp/{OUTPUT_DUMP_FILE}");
    if Path::new(&output_file).exists() {
        fs::remove_file(&output_file)?;
    }

    let instance = CompositorInstance::start(None);
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

    instance.send_request("start", json!({}))?;

    thread::sleep(REGISTER_AT);

    register_mp4_output(&instance, &output_file, None)?;

    instance.send_request(
        "output/output_1/unregister",
        json!({
            "schedule_time_ms": UNREGISTER_AT.as_millis()
        }),
    )?;

    wait_for_output_done(&msg_receiver, "output_1");

    let dump = Bytes::from(fs::read(&output_file)?);
    // Approximate: the output starts once the register request is handled, so it is
    // late by the request roundtrip on top of the sleep. That is well inside TOLERANCE.
    let expected = UNREGISTER_AT - REGISTER_AT;

    check_duration("video", video_duration(&dump)?, expected)?;
    check_duration("audio", audio_duration(&dump)?, expected)?;

    // Same as in `mp4_output_start_at`, except PTS 0 here is the input at whatever
    // point the register request landed, so it drifts by a frame or two between runs.
    compare_video_dumps(
        OUTPUT_DUMP_FILE,
        &dump,
        VideoCompareConfig {
            validation_intervals: vec![Duration::ZERO..Duration::from_secs(7)],
            max_failed_pairs: 10,
            ..Default::default()
        },
    )?;

    let mut fft_cfg = FftCompareConfig::real(vec![Duration::ZERO..Duration::from_secs(7)]);
    fft_cfg.mode = Mode::Real(RealTolerance {
        max_frequency_level: 5.0,
        average_level: 15.0,
        median_level: 15.0,
        general_level: 5.0,
        ..Default::default()
    });

    compare_audio_dumps(
        OUTPUT_DUMP_FILE,
        &dump,
        AudioCompareConfig {
            fft: Some(fft_cfg),
            ..Default::default()
        },
    )?;

    Ok(())
}

fn register_mp4_output(
    instance: &CompositorInstance,
    output_file: &str,
    start_at: Option<Duration>,
) -> Result<()> {
    let mut request = json!({
        "type": "mp4",
        "path": output_file,
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
                    "type": "rescaler",
                    "child": {
                        "type": "input_stream",
                        "input_id": "input_1"
                    }
                }
            }
        },
        "audio": {
            "channels": "stereo",
            "encoder": {
                "type": "aac",
                // The audio decode path runs at 48 kHz; the default
                // for MP4 outputs is 44.1 kHz.
                "sample_rate": 48000,
            },
            "initial": {
                "inputs": [{ "input_id": "input_1" }]
            }
        }
    });
    if let Some(start_at) = start_at {
        request["start_at_ms"] = json!(start_at.as_millis());
    }

    instance.send_request("output/output_1/register", request)?;
    Ok(())
}

/// Allowed drift on the produced media duration. The queue can be a frame or two off
/// around the scheduled start and unregister PTS; it cannot be off by anything close
/// to the `start_at` offset the tests measure.
const TOLERANCE: Duration = Duration::from_millis(750);

fn wait_for_output_done(receiver: &Receiver<tungstenite::Message>, output_id: &str) {
    let expected = format!("\"type\":\"OUTPUT_DONE\",\"output_id\":\"{output_id}\"");
    for msg in receiver.iter() {
        if let tungstenite::Message::Text(msg) = msg
            && msg.contains(&expected)
        {
            return;
        }
    }
}

/// PTS of the last decoded video frame. `Mp4VideoFrameSource` normalizes the first
/// frame to zero, so this is the duration of the track minus its final frame.
fn video_duration(dump: &Bytes) -> Result<Duration> {
    let mut source = Mp4VideoFrameSource::from_bytes(dump)?;
    let mut frames: Vec<Frame> = Vec::new();
    while let Some(batch) = source.next_batch()? {
        frames.extend(batch);
    }
    match frames.last() {
        Some(frame) => Ok(frame.pts),
        None => bail!("MP4 output has no video frames"),
    }
}

/// Audio counterpart of [`video_duration`]; `decode_aac_audio` normalizes the same way.
fn audio_duration(dump: &Bytes) -> Result<Duration> {
    let batches = decode_aac_audio(dump, 48_000)?;
    match batches.last() {
        Some(batch) => Ok(batch.pts),
        None => bail!("MP4 output has no audio samples"),
    }
}

fn check_duration(track: &str, actual: Duration, expected: Duration) -> Result<()> {
    if actual.abs_diff(expected) > TOLERANCE {
        bail!(
            "{track}: MP4 output holds {actual:.2?} of media, expected {expected:.2?} \
             (tolerance {TOLERANCE:?})"
        );
    }
    Ok(())
}
