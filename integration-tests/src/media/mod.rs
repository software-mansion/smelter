//! Unified helpers for sending/receiving media during examples and tests.
//!
//! High-level API:
//!
//! ```ignore
//! use integration_tests::media::*;
//!
//! // Send a built-in sample over RTP (IP defaults to 127.0.0.1)
//! MediaSender::new(
//!     TestSample::BigBuckBunnyH264Opus,
//!     Send::rtp_udp_client().video_port(5000).audio_port(5002),
//! )
//! .spawn()?;
//!
//! // Receive RTP H264
//! MediaReceiver::new(Receive::rtp_udp_listener().video(6000, VideoCodec::H264)).spawn()?;
//!
//! // Listen for an RTMP push
//! MediaReceiver::new(Receive::rtmp_listener(1935)).spawn()?;
//!
//! // Send a local file, loop, via gstreamer; inherit stdio
//! MediaSender::new(
//!     "path/to/file.mp4",
//!     Send::rtp_udp_client().video_port(5000),
//! )
//! .with_backend(Backend::Gstreamer)
//! .with_looped_input(true)
//! .with_stdio(true)
//! .spawn()?;
//! ```

use anyhow::{Result, anyhow};
use smelter_api::Resolution;
use std::{
    fs::{self, File},
    io,
    path::{Path, PathBuf},
};
use tracing::{info, warn};

use crate::paths::integration_tests_root;

mod ffmpeg;
mod gstreamer;
mod handle;
mod sdp;

pub use handle::ProcessHandle;

// ---------------------------------------------------------------------------
// Codec and backend enums
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VideoCodec {
    H264,
    Vp8,
    Vp9,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AudioCodec {
    Opus,
    Aac,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Backend {
    #[default]
    Ffmpeg,
    Gstreamer,
}

// ---------------------------------------------------------------------------
// Built-in samples
// ---------------------------------------------------------------------------

/// Built-in media files. Resolved lazily — downloaded on first use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TestSample {
    BigBuckBunnyH264Opus,
    BigBuckBunnyH264AAC,
    BigBuckBunnyVP8Opus,
    BigBuckBunnyVP9Opus,
    ElephantsDreamH264Opus,
    ElephantsDreamVP8Opus,
    ElephantsDreamVP9Opus,
    OceanSampleH264,
    OceanSampleVP8,
    OceanSampleVP9,
}

struct SampleInfo {
    url: &'static str,
    /// Path relative to the integration-tests crate root.
    path: &'static str,
    video: VideoCodec,
    audio: Option<AudioCodec>,
}

const SAMPLES: &[(TestSample, SampleInfo)] = &[
    (
        TestSample::BigBuckBunnyH264Opus,
        SampleInfo {
            url: "https://github.com/smelter-labs/smelter-snapshot-tests/raw/refs/heads/main/assets/BigBuckBunny720p24fps490s.mp4",
            path: "examples/assets/BigBuckBunny720p24fps490s.mp4",
            video: VideoCodec::H264,
            audio: Some(AudioCodec::Opus),
        },
    ),
    (
        // Same file as BigBuckBunnyH264Opus — the mp4 actually has AAC audio.
        TestSample::BigBuckBunnyH264AAC,
        SampleInfo {
            url: "https://github.com/smelter-labs/smelter-snapshot-tests/raw/refs/heads/main/assets/BigBuckBunny720p24fps490s.mp4",
            path: "examples/assets/BigBuckBunny720p24fps490s.mp4",
            video: VideoCodec::H264,
            audio: Some(AudioCodec::Aac),
        },
    ),
    (
        TestSample::BigBuckBunnyVP8Opus,
        SampleInfo {
            url: "https://github.com/smelter-labs/smelter-snapshot-tests/raw/refs/heads/main/assets/BigBuckBunny720p24fps60s.vp8.webm",
            path: "examples/assets/BigBuckBunny720p24fps60s.vp8.webm",
            video: VideoCodec::Vp8,
            audio: Some(AudioCodec::Opus),
        },
    ),
    (
        TestSample::BigBuckBunnyVP9Opus,
        SampleInfo {
            url: "https://github.com/smelter-labs/smelter-snapshot-tests/raw/refs/heads/main/assets/BigBuckBunny720p24fps60s.vp9.webm",
            path: "examples/assets/BigBuckBunny720p24fps60s.vp9.webm",
            video: VideoCodec::Vp9,
            audio: Some(AudioCodec::Opus),
        },
    ),
    (
        TestSample::ElephantsDreamH264Opus,
        SampleInfo {
            url: "https://github.com/smelter-labs/smelter-snapshot-tests/raw/refs/heads/main/assets/ElephantsDream720p24fps60s.mp4",
            path: "examples/assets/ElephantsDream720p24fps60s.mp4",
            video: VideoCodec::H264,
            audio: Some(AudioCodec::Opus),
        },
    ),
    (
        TestSample::ElephantsDreamVP8Opus,
        SampleInfo {
            url: "https://github.com/smelter-labs/smelter-snapshot-tests/raw/refs/heads/main/assets/ElephantsDream720p24fps60s.vp8.webm",
            path: "examples/assets/ElephantsDream720p24fps60s.vp8.webm",
            video: VideoCodec::Vp8,
            audio: Some(AudioCodec::Opus),
        },
    ),
    (
        TestSample::ElephantsDreamVP9Opus,
        SampleInfo {
            url: "https://github.com/smelter-labs/smelter-snapshot-tests/raw/refs/heads/main/assets/ElephantsDream720p24fps60s.vp9.webm",
            path: "examples/assets/ElephantsDream720p24fps60s.vp9.webm",
            video: VideoCodec::Vp9,
            audio: Some(AudioCodec::Opus),
        },
    ),
    (
        TestSample::OceanSampleH264,
        SampleInfo {
            url: "https://filesamples.com/samples/video/mp4/sample_1280x720.mp4",
            path: "examples/assets/OceanSample720p24fps28s.mp4",
            video: VideoCodec::H264,
            audio: None,
        },
    ),
    (
        TestSample::OceanSampleVP8,
        SampleInfo {
            url: "https://github.com/smelter-labs/smelter-snapshot-tests/raw/refs/heads/main/assets/OceanSample720p24fps28s.vp8.webm",
            path: "examples/assets/OceanSample720p24fps28s.vp8.webm",
            video: VideoCodec::Vp8,
            audio: None,
        },
    ),
    (
        TestSample::OceanSampleVP9,
        SampleInfo {
            url: "https://github.com/smelter-labs/smelter-snapshot-tests/raw/refs/heads/main/assets/OceanSample720p24fps28s.vp9.webm",
            path: "examples/assets/OceanSample720p24fps28s.vp9.webm",
            video: VideoCodec::Vp9,
            audio: None,
        },
    ),
];

fn sample_info(sample: TestSample) -> &'static SampleInfo {
    SAMPLES
        .iter()
        .find_map(|(s, info)| (*s == sample).then_some(info))
        .expect("all TestSample variants have an entry in SAMPLES")
}

/// Eagerly download every built-in sample. Useful to warm the cache at startup.
pub fn download_all_samples() -> Result<()> {
    for (_, info) in SAMPLES {
        let path = integration_tests_root().join(info.path);
        if let Err(err) = download(info.url, &path) {
            warn!(?path, "Failed to download asset: {err}");
        }
    }
    Ok(())
}

fn download(url: &str, path: &Path) -> Result<()> {
    if path.exists() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    info!("Downloading asset {url} -> {}", path.display());
    let mut resp = reqwest::blocking::get(url)?;
    let mut out = File::create(path)?;
    io::copy(&mut resp, &mut out)?;
    Ok(())
}

/// Low-level: download an arbitrary url to a path relative to the current working dir.
/// Prefer [`TestSample`] or [`Asset::File`] when possible.
pub fn download_to(url: &str, path: impl AsRef<Path>) -> Result<PathBuf> {
    let path = std::env::current_dir()?.join(path);
    if !path.exists() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut resp = reqwest::blocking::get(url)?;
        let mut out = File::create(&path)?;
        io::copy(&mut resp, &mut out)?;
    }
    Ok(path)
}

// ---------------------------------------------------------------------------
// Asset abstraction
// ---------------------------------------------------------------------------

/// The media source for a sender. Construct via [`From`] impls:
///
/// - `TestSample::…` — a built-in sample (auto-downloaded)
/// - `PathBuf` / `&Path` / `&str` — a local file
/// - Use [`Asset::pattern`] for a generated test pattern (no file backing)
#[derive(Clone, Debug)]
pub enum Asset {
    Sample(TestSample),
    File(PathBuf),
    /// Generated test pattern (no file read).
    Pattern {
        video: VideoCodec,
        resolution: Resolution,
    },
}

impl Asset {
    pub fn pattern(video: VideoCodec, resolution: Resolution) -> Self {
        Asset::Pattern { video, resolution }
    }

    /// Ensures the asset is available on disk (downloads samples if missing)
    /// and reports codec metadata for file-backed variants.
    pub fn resolve(&self) -> Result<ResolvedAsset> {
        match self {
            Asset::Sample(sample) => {
                let info = sample_info(*sample);
                let path = integration_tests_root().join(info.path);
                download(info.url, &path)?;
                Ok(ResolvedAsset {
                    kind: ResolvedKind::File(path),
                    video: Some(info.video),
                    audio: info.audio,
                })
            }
            Asset::File(path) => {
                if !path.exists() {
                    return Err(anyhow!("asset not found: {}", path.display()));
                }
                Ok(ResolvedAsset {
                    kind: ResolvedKind::File(path.clone()),
                    video: None,
                    audio: None,
                })
            }
            Asset::Pattern { video, resolution } => Ok(ResolvedAsset {
                kind: ResolvedKind::Pattern {
                    video: *video,
                    resolution: *resolution,
                },
                video: Some(*video),
                audio: None,
            }),
        }
    }
}

impl From<TestSample> for Asset {
    fn from(s: TestSample) -> Self {
        Asset::Sample(s)
    }
}
impl From<PathBuf> for Asset {
    fn from(p: PathBuf) -> Self {
        Asset::File(p)
    }
}
impl From<&Path> for Asset {
    fn from(p: &Path) -> Self {
        Asset::File(p.to_path_buf())
    }
}
impl From<&PathBuf> for Asset {
    fn from(p: &PathBuf) -> Self {
        Asset::File(p.clone())
    }
}
impl From<&str> for Asset {
    fn from(p: &str) -> Self {
        Asset::File(PathBuf::from(p))
    }
}
impl From<String> for Asset {
    fn from(p: String) -> Self {
        Asset::File(PathBuf::from(p))
    }
}

pub struct ResolvedAsset {
    pub kind: ResolvedKind,
    pub video: Option<VideoCodec>,
    pub audio: Option<AudioCodec>,
}

pub enum ResolvedKind {
    File(PathBuf),
    Pattern {
        video: VideoCodec,
        resolution: Resolution,
    },
}

impl ResolvedAsset {
    pub fn path(&self) -> Option<&Path> {
        match &self.kind {
            ResolvedKind::File(p) => Some(p.as_path()),
            ResolvedKind::Pattern { .. } => None,
        }
    }
}

impl TestSample {
    /// URL the sample is downloaded from.
    pub fn url(self) -> &'static str {
        sample_info(self).url
    }

    /// On-disk path of the sample. The file is expected to be already downloaded
    /// (either by `run_example`/`run_example_server` or an explicit call to
    /// [`download_all_samples`]); senders/receivers also download on demand via
    /// [`Asset::resolve`].
    pub fn file(self) -> PathBuf {
        integration_tests_root().join(sample_info(self).path)
    }

    /// Video codec of the sample.
    pub fn video_codec(self) -> VideoCodec {
        sample_info(self).video
    }

    /// Audio codec of the sample, if it has audio.
    pub fn audio_codec(self) -> Option<AudioCodec> {
        sample_info(self).audio
    }
}

// ---------------------------------------------------------------------------
// Protocol specs
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub enum Send {
    /// RTP over UDP. When both ports are None an error is returned on spawn.
    RtpUdpClient {
        ip: String,
        video_port: Option<u16>,
        audio_port: Option<u16>,
    },
    /// RTP over TCP (GStreamer only).
    RtpTcpClient {
        ip: String,
        video_port: Option<u16>,
        audio_port: Option<u16>,
    },
    /// RTMP push to a full URL.
    RtmpClient { url: String },
}

impl Send {
    /// Builder for RTP-over-UDP sending. Defaults: IP `127.0.0.1`, no ports.
    pub fn rtp_udp_client() -> SendRtpUdp {
        SendRtpUdp::default()
    }
    /// Builder for RTP-over-TCP sending. Defaults: IP `127.0.0.1`, no ports.
    pub fn rtp_tcp_client() -> SendRtpTcp {
        SendRtpTcp::default()
    }
    pub fn rtmp_client(url: impl Into<String>) -> Self {
        Send::RtmpClient { url: url.into() }
    }
}

/// Builder for [`Send::RtpUdpClient`]. Consumed by [`MediaSender::new`] via `Into<Send>`.
#[derive(Clone, Debug)]
pub struct SendRtpUdp {
    ip: String,
    video_port: Option<u16>,
    audio_port: Option<u16>,
}

impl Default for SendRtpUdp {
    fn default() -> Self {
        Self {
            ip: "127.0.0.1".to_string(),
            video_port: None,
            audio_port: None,
        }
    }
}

impl SendRtpUdp {
    pub fn ip(mut self, ip: impl Into<String>) -> Self {
        self.ip = ip.into();
        self
    }
    pub fn video_port(mut self, port: impl Into<Option<u16>>) -> Self {
        self.video_port = port.into();
        self
    }
    pub fn audio_port(mut self, port: impl Into<Option<u16>>) -> Self {
        self.audio_port = port.into();
        self
    }
}

impl From<SendRtpUdp> for Send {
    fn from(b: SendRtpUdp) -> Self {
        Send::RtpUdpClient {
            ip: b.ip,
            video_port: b.video_port,
            audio_port: b.audio_port,
        }
    }
}

/// Builder for [`Send::RtpTcpClient`]. Consumed by [`MediaSender::new`] via `Into<Send>`.
#[derive(Clone, Debug)]
pub struct SendRtpTcp {
    ip: String,
    video_port: Option<u16>,
    audio_port: Option<u16>,
}

impl Default for SendRtpTcp {
    fn default() -> Self {
        Self {
            ip: "127.0.0.1".to_string(),
            video_port: None,
            audio_port: None,
        }
    }
}

impl SendRtpTcp {
    pub fn ip(mut self, ip: impl Into<String>) -> Self {
        self.ip = ip.into();
        self
    }
    pub fn video_port(mut self, port: impl Into<Option<u16>>) -> Self {
        self.video_port = port.into();
        self
    }
    pub fn audio_port(mut self, port: impl Into<Option<u16>>) -> Self {
        self.audio_port = port.into();
        self
    }
}

impl From<SendRtpTcp> for Send {
    fn from(b: SendRtpTcp) -> Self {
        Send::RtpTcpClient {
            ip: b.ip,
            video_port: b.video_port,
            audio_port: b.audio_port,
        }
    }
}

#[derive(Clone, Debug)]
pub struct RtpVideo {
    pub port: u16,
    pub codec: VideoCodec,
}

/// Describes how a receiver is configured. Variant names call out whether the
/// receiver *listens* for an incoming connection or *connects* out to a source.
#[derive(Clone, Debug)]
pub enum Receive {
    /// Bind a UDP socket and receive RTP packets.
    RtpUdpListener {
        video: Option<RtpVideo>,
        audio_port: Option<u16>,
    },
    /// Connect (TCP client) to a remote RTP-over-TCP server.
    RtpTcpClient {
        ip: String,
        video: Option<RtpVideo>,
        audio_port: Option<u16>,
    },
    /// Listen for an incoming RTMP push.
    RtmpListener { port: u16 },
    /// Play an HLS playlist from disk.
    HlsPlayer { playlist: PathBuf },
}

impl Receive {
    /// Builder for RTP-over-UDP listening. Defaults: no video, no audio port.
    pub fn rtp_udp_listener() -> ReceiveRtpUdp {
        ReceiveRtpUdp::default()
    }
    /// Builder for RTP-over-TCP client receive. Defaults: IP `127.0.0.1`, no streams.
    pub fn rtp_tcp_client() -> ReceiveRtpTcp {
        ReceiveRtpTcp::default()
    }
    pub fn rtmp_listener(port: u16) -> Self {
        Receive::RtmpListener { port }
    }
    pub fn hls_player(playlist: impl Into<PathBuf>) -> Self {
        Receive::HlsPlayer {
            playlist: playlist.into(),
        }
    }
}

/// Builder for [`Receive::RtpUdpListener`]. Consumed by [`MediaReceiver::new`] via `Into<Receive>`.
#[derive(Clone, Debug, Default)]
pub struct ReceiveRtpUdp {
    video: Option<RtpVideo>,
    audio_port: Option<u16>,
}

impl ReceiveRtpUdp {
    pub fn video(mut self, port: u16, codec: VideoCodec) -> Self {
        self.video = Some(RtpVideo { port, codec });
        self
    }
    pub fn audio_port(mut self, port: impl Into<Option<u16>>) -> Self {
        self.audio_port = port.into();
        self
    }
}

impl From<ReceiveRtpUdp> for Receive {
    fn from(b: ReceiveRtpUdp) -> Self {
        Receive::RtpUdpListener {
            video: b.video,
            audio_port: b.audio_port,
        }
    }
}

/// Builder for [`Receive::RtpTcpClient`]. Consumed by [`MediaReceiver::new`] via `Into<Receive>`.
#[derive(Clone, Debug)]
pub struct ReceiveRtpTcp {
    ip: String,
    video: Option<RtpVideo>,
    audio_port: Option<u16>,
}

impl Default for ReceiveRtpTcp {
    fn default() -> Self {
        Self {
            ip: "127.0.0.1".to_string(),
            video: None,
            audio_port: None,
        }
    }
}

impl ReceiveRtpTcp {
    pub fn ip(mut self, ip: impl Into<String>) -> Self {
        self.ip = ip.into();
        self
    }
    pub fn video(mut self, port: u16, codec: VideoCodec) -> Self {
        self.video = Some(RtpVideo { port, codec });
        self
    }
    pub fn audio_port(mut self, port: impl Into<Option<u16>>) -> Self {
        self.audio_port = port.into();
        self
    }
}

impl From<ReceiveRtpTcp> for Receive {
    fn from(b: ReceiveRtpTcp) -> Self {
        Receive::RtpTcpClient {
            ip: b.ip,
            video: b.video,
            audio_port: b.audio_port,
        }
    }
}

// ---------------------------------------------------------------------------
// Builder entrypoints
// ---------------------------------------------------------------------------

pub struct MediaSender {
    asset: Asset,
    /// Destination this sender writes to (RTP UDP/TCP, RTMP, …).
    to: Send,
    backend: Backend,
    looped_input: bool,
    stdio: bool,
}

impl MediaSender {
    pub fn new(src: impl Into<Asset>, to: impl Into<Send>) -> Self {
        Self {
            asset: src.into(),
            to: to.into(),
            backend: Backend::default(),
            looped_input: false,
            stdio: false,
        }
    }
    pub fn with_backend(mut self, backend: Backend) -> Self {
        self.backend = backend;
        self
    }
    pub fn with_looped_input(mut self, looped_input: bool) -> Self {
        self.looped_input = looped_input;
        self
    }
    /// Inherit stdout/stderr from the parent process. Default: false (piped to /dev/null).
    pub fn with_stdio(mut self, stdio: bool) -> Self {
        self.stdio = stdio;
        self
    }
    pub fn spawn(self) -> Result<Vec<ProcessHandle>> {
        let resolved = self.asset.resolve()?;
        match self.backend {
            Backend::Ffmpeg => {
                ffmpeg::spawn_send(&resolved, &self.to, self.looped_input, self.stdio)
            }
            Backend::Gstreamer => {
                gstreamer::spawn_send(&resolved, &self.to, self.looped_input, self.stdio)
            }
        }
    }
}

pub struct MediaReceiver {
    from: Receive,
    backend: Backend,
    stdio: bool,
}

impl MediaReceiver {
    pub fn new(from: impl Into<Receive>) -> Self {
        Self {
            from: from.into(),
            backend: Backend::default(),
            stdio: false,
        }
    }
    pub fn with_backend(mut self, backend: Backend) -> Self {
        self.backend = backend;
        self
    }
    /// Inherit stdout/stderr from the parent process. Default: false (piped to /dev/null).
    pub fn with_stdio(mut self, stdio: bool) -> Self {
        self.stdio = stdio;
        self
    }
    pub fn spawn(self) -> Result<Vec<ProcessHandle>> {
        match self.backend {
            Backend::Ffmpeg => ffmpeg::spawn_receive(&self.from, self.stdio),
            Backend::Gstreamer => gstreamer::spawn_receive(&self.from, self.stdio),
        }
    }
}
