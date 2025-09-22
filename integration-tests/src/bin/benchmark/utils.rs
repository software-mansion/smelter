use anyhow::Result;
use integration_tests::paths::integration_tests_root;
use smelter_core::{
    DEFAULT_BUFFER_DURATION, PipelineOptions, PipelineWgpuOptions, PipelineWhipWhepServerOptions,
    graphics_context::GraphicsContext,
};
use std::{
    fs::{self, File},
    io::{self, Read},
    path::PathBuf,
    sync::Arc,
    time::Duration,
};
use tracing::warn;

use smelter_render::{Framerate, RenderingMode, YuvPlanes};

use crate::{args::Resolution, benchmark_pass::RawInputFile};

pub fn benchmark_pipeline_options(
    framerate: u64,
    graphics_context: GraphicsContext,
    rendering_mode: RenderingMode,
) -> PipelineOptions {
    PipelineOptions {
        never_drop_output_frames: true,
        output_framerate: Framerate {
            num: framerate as u32,
            den: 1,
        },
        default_buffer_duration: DEFAULT_BUFFER_DURATION,
        ahead_of_time_processing: false,
        run_late_scheduled_events: true,
        chromium_context: None,
        download_root: std::env::temp_dir().into(),
        load_system_fonts: false,
        mixing_sample_rate: 48_000,
        stream_fallback_timeout: Duration::from_millis(500),
        tokio_rt: None,
        whip_whep_stun_servers: Vec::new().into(),
        rendering_mode,
        whip_whep_server: PipelineWhipWhepServerOptions::Disable,
        wgpu_options: PipelineWgpuOptions::Context(graphics_context),
    }
}

pub fn generate_yuv_from_mp4(source: &PathBuf) -> Result<RawInputFile, String> {
    let destination = std::path::PathBuf::from(format!(
        "/tmp/smelter_benchmark_input_{}.yuv",
        std::time::UNIX_EPOCH.elapsed().unwrap().as_millis()
    ));
    let duration = Duration::from_secs(30);

    let mut probe_cmd = std::process::Command::new("ffprobe");
    probe_cmd
        .arg("-v")
        .arg("error")
        .arg("-select_streams")
        .arg("v:0")
        .arg("-show_entries")
        .arg("stream=width,height,r_frame_rate")
        .arg("-of")
        .arg("csv=p=0")
        .arg(source)
        .stdout(std::process::Stdio::piped());

    let probe_result = probe_cmd
        .spawn()
        .map_err(|err| err.to_string())?
        .wait_with_output()
        .map_err(|err| err.to_string())?;

    if !probe_result.status.success() {
        return Err("ffprobe command failed".into());
    }

    let probe_result = String::from_utf8(probe_result.stdout).unwrap();
    let mut probe_parts = probe_result.trim().split(",");
    let width = probe_parts.next().unwrap().parse::<usize>().unwrap();
    let height = probe_parts.next().unwrap().parse::<usize>().unwrap();
    let mut framerate = probe_parts.next().unwrap().split("/");
    let num = framerate.next().unwrap().parse::<u32>().unwrap();
    let den = framerate.next().unwrap().parse::<u32>().unwrap();

    let mut convert_cmd = std::process::Command::new("ffmpeg");
    convert_cmd
        .arg("-i")
        .arg(source)
        .arg("-pix_fmt")
        .arg("yuv420p")
        .arg("-t")
        .arg((duration.mul_f64(1.5)).as_secs().to_string())
        .arg(&destination)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());

    let mut ffmpeg = convert_cmd.spawn().map_err(|e| e.to_string())?;
    let status = ffmpeg
        .wait()
        .expect("wait for ffmpeg to finish yuv conversion");

    if !status.success() {
        return Err("ffmpeg for yuv conversion terminated unsuccessfully".into());
    }

    let resolution = Resolution { width, height };
    let frames = read_frames(&destination, 300, resolution);
    if frames.len() < 50 {
        panic!("Video should have at least 50 frames")
    }

    let _ = fs::remove_file(&destination);

    Ok(RawInputFile {
        frames: Arc::new(frames),
        resolution,
        framerate: num as f64 / den as f64,
    })
}

fn read_frames(path: &PathBuf, count: usize, resolution: Resolution) -> Vec<YuvPlanes> {
    let mut file = File::open(path).unwrap();

    let Resolution { width, height } = resolution;
    let dimensions = width * height;
    let mut buffer = vec![0u8; dimensions * 3 / 2];

    (0..count)
        .map_while(|_| {
            let Ok(()) = file.read_exact(&mut buffer) else {
                return None;
            };
            let y_plane = &buffer[..dimensions];
            let u_plane = &buffer[dimensions..dimensions * 5 / 4];
            let v_plane = &buffer[dimensions * 5 / 4..];
            Some(YuvPlanes {
                y_plane: bytes::Bytes::from(y_plane.to_vec()),
                u_plane: bytes::Bytes::from(u_plane.to_vec()),
                v_plane: bytes::Bytes::from(v_plane.to_vec()),
            })
        })
        .collect()
}

const DEFAULT_MP4_URL: &str =
    "https://commondatastorage.googleapis.com/gtv-videos-bucket/sample/BigBuckBunny.mp4";

const BBB_720P_24FPS: &str = "https://github.com/smelter-labs/smelter-snapshot-tests/raw/refs/heads/main/assets/BigBuckBunny720p24fps30s.mp4";
const BBB_1080P_30FPS: &str = "https://github.com/smelter-labs/smelter-snapshot-tests/raw/refs/heads/main/assets/BigBuckBunny1080p30fps30s.mp4";
const BBB_1080P_60FPS: &str = "https://github.com/smelter-labs/smelter-snapshot-tests/raw/refs/heads/main/assets/BigBuckBunny1080p60fps30s.mp4";
const BBB_2160P_30FPS: &str = "https://github.com/smelter-labs/smelter-snapshot-tests/raw/refs/heads/main/assets/BigBuckBunny2160p30fps30s.mp4";

pub fn ensure_bunny_720p24fps() -> Result<PathBuf> {
    let path = integration_tests_root().join("./examples/assets/BigBuckBunny720p24fps30s.mp4");
    ensure_file(path, BBB_720P_24FPS)
}

pub fn ensure_bunny_1080p30fps() -> Result<PathBuf> {
    let path = integration_tests_root().join("./examples/assets/BigBuckBunny1080p30fps30s.mp4");
    ensure_file(path, BBB_1080P_30FPS)
}

pub fn ensure_bunny_1080p60fps() -> Result<PathBuf> {
    let path = integration_tests_root().join("./examples/assets/BigBuckBunny1080p60fps30s.mp4");
    ensure_file(path, BBB_1080P_60FPS)
}

pub fn ensure_bunny_2160p30fps() -> Result<PathBuf> {
    let path = integration_tests_root().join("./examples/assets/BigBuckBunny2160p30fps30s.mp4");
    ensure_file(path, BBB_2160P_30FPS)
}

pub fn ensure_default_mp4() -> Result<PathBuf> {
    let path = integration_tests_root().join("./examples/assets/BigBuckBunny.mp4");
    ensure_file(path, DEFAULT_MP4_URL)
}

fn ensure_file(path: PathBuf, url: &str) -> Result<PathBuf> {
    fs::create_dir_all(path.parent().unwrap())?;
    if !path.exists() {
        warn!(?path, ?url, "Downloading asset");
        let mut resp = reqwest::blocking::get(url)?;
        let mut out = File::create(path.clone())?;
        io::copy(&mut resp, &mut out)?;
    }
    Ok(path)
}

pub fn example_image_path() -> PathBuf {
    integration_tests_root().join("./examples/assets/image.png")
}

pub fn generate_png_from_video(source: &PathBuf) {
    let dest = example_image_path();
    if dest.exists() {
        return;
    }
    let mut convert_cmd = std::process::Command::new("ffmpeg");
    convert_cmd
        .arg("-i")
        .arg(source)
        .args(["-ss", "00:00:05.00", "-vframes", "1"])
        .arg(dest)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap()
        .wait()
        .unwrap();
}
