use anyhow::{Result, anyhow};
use std::{
    path::Path,
    process::{Command, Stdio},
    thread,
    time::Duration,
};
use tracing::info;

use super::{
    AudioCodec, Receive, ResolvedAsset, ResolvedKind, Send, VideoCodec, handle::ProcessHandle,
    sdp::write_sdp,
};

pub(super) fn spawn_send(
    asset: &ResolvedAsset,
    to: &Send,
    looped_input: bool,
    stdio: bool,
) -> Result<Vec<ProcessHandle>> {
    match to {
        Send::RtpUdpClient {
            ip,
            video_port,
            audio_port,
        } => send_rtp_udp(asset, ip, *video_port, *audio_port, looped_input, stdio),
        Send::RtpTcpClient { .. } => Err(anyhow!(
            "FFmpeg backend does not support RTP TCP send; use Backend::Gstreamer"
        )),
        Send::RtmpClient { url } => send_rtmp(asset, url, stdio),
    }
}

pub(super) fn spawn_receive(from: &Receive, stdio: bool) -> Result<Vec<ProcessHandle>> {
    match from {
        Receive::RtpUdpListener { video, audio_port } => {
            receive_rtp_udp(video.as_ref(), *audio_port, stdio)
        }
        Receive::RtpTcpClient { .. } => Err(anyhow!(
            "FFmpeg backend does not support RTP TCP receive; use Backend::Gstreamer"
        )),
        Receive::RtmpListener { port } => receive_rtmp(*port, stdio),
        Receive::HlsPlayer { playlist } => receive_hls(playlist, stdio),
    }
}

fn stdio_for(stdio: bool) -> (Stdio, Stdio) {
    if stdio {
        (Stdio::inherit(), Stdio::inherit())
    } else {
        (Stdio::null(), Stdio::null())
    }
}

// ---------------------------------------------------------------------------
// Send: RTP UDP
// ---------------------------------------------------------------------------

fn send_rtp_udp(
    asset: &ResolvedAsset,
    ip: &str,
    video_port: Option<u16>,
    audio_port: Option<u16>,
    looped_input: bool,
    stdio: bool,
) -> Result<Vec<ProcessHandle>> {
    if video_port.is_none() && audio_port.is_none() {
        return Err(anyhow!("At least one of video_port/audio_port must be set"));
    }

    let mut handles = Vec::new();
    match &asset.kind {
        ResolvedKind::File(path) => {
            if let Some(port) = video_port {
                let codec = asset.video.ok_or_else(|| {
                    anyhow!(
                        "video codec unknown for file asset; use a TestSample or select gstreamer"
                    )
                })?;
                handles.push(send_video_from_file(
                    ip,
                    port,
                    path,
                    codec,
                    looped_input,
                    stdio,
                )?);
            }
            if let Some(port) = audio_port {
                let audio_codec_str = match asset.audio {
                    Some(AudioCodec::Aac) => "copy",
                    _ => "libopus",
                };
                handles.push(send_audio_from_file(
                    ip,
                    port,
                    path,
                    audio_codec_str,
                    stdio,
                )?);
            }
        }
        ResolvedKind::Pattern { video, resolution } => {
            let port = video_port.ok_or_else(|| anyhow!("test pattern requires video_port"))?;
            handles.push(send_testsrc(ip, port, *video, *resolution, stdio)?);
            if audio_port.is_some() {
                return Err(anyhow!(
                    "FFmpeg testsrc pattern doesn't emit audio; use gstreamer for audio test pattern"
                ));
            }
        }
    }
    Ok(handles)
}

fn send_video_from_file(
    ip: &str,
    port: u16,
    path: &Path,
    codec: VideoCodec,
    looped_input: bool,
    stdio: bool,
) -> Result<ProcessHandle> {
    info!("[media] ffmpeg: sending video to {ip}:{port} (loop={looped_input})");

    let codec_args: &[&str] = match codec {
        VideoCodec::H264 => &["-bsf:v", "h264_mp4toannexb"],
        VideoCodec::Vp8 => &[],
        VideoCodec::Vp9 => &["-strict", "experimental"],
    };

    let (out, err) = stdio_for(stdio);
    let mut cmd = Command::new("ffmpeg");
    if looped_input {
        cmd.args(["-stream_loop", "-1"]);
    }
    cmd.args(["-re", "-i"])
        .arg(path)
        .args(["-an", "-c:v", "copy", "-f", "rtp"])
        .args(codec_args)
        .arg(format!("rtp://{ip}:{port}?rtcpport={port}"))
        .stdout(out)
        .stderr(err);

    Ok(ProcessHandle::new(cmd.spawn()?))
}

fn send_audio_from_file(
    ip: &str,
    port: u16,
    path: &Path,
    codec: &str,
    stdio: bool,
) -> Result<ProcessHandle> {
    info!("[media] ffmpeg: sending audio to {ip}:{port} ({codec})");
    let (out, err) = stdio_for(stdio);
    let child = Command::new("ffmpeg")
        .args(["-stream_loop", "-1", "-re", "-i"])
        .arg(path)
        .args([
            "-vn",
            "-c:a",
            codec,
            "-f",
            "rtp",
            &format!("rtp://{ip}:{port}?rtcpport={port}"),
        ])
        .stdout(out)
        .stderr(err)
        .spawn()?;
    Ok(ProcessHandle::new(child))
}

fn send_testsrc(
    ip: &str,
    port: u16,
    codec: VideoCodec,
    resolution: smelter_api::Resolution,
    stdio: bool,
) -> Result<ProcessHandle> {
    info!("[media] ffmpeg: sending test pattern to {ip}:{port}");

    let src = format!(
        "testsrc=s={}x{}:r=30,format=yuv420p",
        resolution.width, resolution.height
    );
    let codec_args: Vec<&str> = match codec {
        VideoCodec::H264 => vec!["libx264"],
        VideoCodec::Vp8 => vec![
            "libvpx",
            "-deadline",
            "realtime",
            "-error-resilient",
            "1",
            "-b:v",
            "1M",
        ],
        VideoCodec::Vp9 => vec![
            "libvpx-vp9",
            "-deadline",
            "realtime",
            "-error-resilient",
            "1",
            "-b:v",
            "1M",
            "-strict",
            "experimental",
        ],
    };

    let (out, err) = stdio_for(stdio);
    let child = Command::new("ffmpeg")
        .args(["-re", "-f", "lavfi", "-i", &src, "-c:v"])
        .args(codec_args)
        .args(["-f", "rtp", &format!("rtp://{ip}:{port}?rtcpport={port}")])
        .stdout(out)
        .stderr(err)
        .spawn()?;
    Ok(ProcessHandle::new(child))
}

// ---------------------------------------------------------------------------
// Send: RTMP
// ---------------------------------------------------------------------------

fn send_rtmp(asset: &ResolvedAsset, url: &str, stdio: bool) -> Result<Vec<ProcessHandle>> {
    let path = asset
        .path()
        .ok_or_else(|| anyhow!("RTMP send requires a file asset"))?;
    info!("[media] ffmpeg: RTMP push -> {url}");

    let (out, err) = stdio_for(stdio);
    let child = Command::new("bash")
        .arg("-c")
        .arg(format!(
            "ffmpeg -re -i {} -c copy -f flv {url}",
            shell_escape(path.to_string_lossy().as_ref())
        ))
        .stdin(Stdio::null())
        .stdout(out)
        .stderr(err)
        .spawn()?;
    thread::sleep(Duration::from_secs(2));
    Ok(vec![ProcessHandle::new(child)])
}

fn shell_escape(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

// ---------------------------------------------------------------------------
// Receive: RTP UDP
// ---------------------------------------------------------------------------

fn receive_rtp_udp(
    video: Option<&super::RtpVideo>,
    audio_port: Option<u16>,
    stdio: bool,
) -> Result<Vec<ProcessHandle>> {
    if video.is_none() && audio_port.is_none() {
        return Err(anyhow!(
            "At least one of: video, audio_port has to be specified."
        ));
    }
    if let (Some(v), Some(a)) = (video, audio_port)
        && v.port == a
    {
        return Err(anyhow!(
            "FFmpeg can't handle both audio and video on a single port over RTP."
        ));
    }

    let sdp = write_sdp(video.map(|v| (v.port, v.codec)), audio_port)?;
    info!("[media] ffmpeg: receiving via sdp {}", sdp.display());

    let (out, err) = stdio_for(stdio);
    let child = Command::new("ffplay")
        .args(["-protocol_whitelist", "file,rtp,udp"])
        .arg(&sdp)
        .stdout(out)
        .stderr(err)
        .spawn()?;
    thread::sleep(Duration::from_secs(2));
    Ok(vec![ProcessHandle::new(child)])
}

// ---------------------------------------------------------------------------
// Receive: RTMP
// ---------------------------------------------------------------------------

fn receive_rtmp(port: u16, stdio: bool) -> Result<Vec<ProcessHandle>> {
    info!("[media] ffmpeg: RTMP listen on {port}");
    let (out, err) = stdio_for(stdio);
    let child = Command::new("bash")
        .arg("-c")
        .arg(format!(
            "ffmpeg -f flv -listen 1 -i rtmp://0.0.0.0:{port} -vcodec copy -f flv - | ffplay -autoexit -f flv -i -"
        ))
        .stdin(Stdio::null())
        .stdout(out)
        .stderr(err)
        .spawn()?;
    thread::sleep(Duration::from_secs(2));
    Ok(vec![ProcessHandle::new(child)])
}

// ---------------------------------------------------------------------------
// Receive: HLS
// ---------------------------------------------------------------------------

fn receive_hls(playlist: &Path, stdio: bool) -> Result<Vec<ProcessHandle>> {
    for _ in 0..20 {
        if playlist.exists() && !std::fs::read_to_string(playlist)?.trim().is_empty() {
            break;
        }
        thread::sleep(Duration::from_secs(1));
    }
    if !playlist.exists() || std::fs::read_to_string(playlist)?.trim().is_empty() {
        return Err(anyhow!("Playlist file does not exist: {playlist:?}"));
    }

    let (out, err) = stdio_for(stdio);
    let child = Command::new("ffplay")
        .args(["-i"])
        .arg(playlist)
        .stdout(out)
        .stderr(err)
        .spawn()?;
    thread::sleep(Duration::from_secs(2));
    Ok(vec![ProcessHandle::new(child)])
}
