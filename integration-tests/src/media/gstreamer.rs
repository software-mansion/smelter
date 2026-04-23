use anyhow::{Result, anyhow};
use std::{
    path::Path,
    process::{Command, Stdio},
    thread,
    time::Duration,
};
use tracing::info;

use super::{
    Receive, ResolvedAsset, ResolvedKind, RtpVideo, Send, VideoCodec, handle::ProcessHandle,
};

pub(super) fn spawn_send(
    asset: &ResolvedAsset,
    to: &Send,
    loop_input: bool,
    stdio: bool,
) -> Result<Vec<ProcessHandle>> {
    if loop_input {
        return Err(anyhow!(
            "GStreamer backend doesn't support loop_input; use Backend::Ffmpeg"
        ));
    }
    match to {
        Send::RtpUdpClient {
            ip,
            video_port,
            audio_port,
        } => send_rtp(
            asset,
            ip,
            *video_port,
            *audio_port,
            RtpTransport::Udp,
            stdio,
        ),
        Send::RtpTcpClient {
            ip,
            video_port,
            audio_port,
        } => send_rtp(
            asset,
            ip,
            *video_port,
            *audio_port,
            RtpTransport::Tcp,
            stdio,
        ),
        Send::RtmpClient { .. } => Err(anyhow!(
            "GStreamer backend doesn't support RTMP send; use Backend::Ffmpeg"
        )),
    }
}

pub(super) fn spawn_receive(from: &Receive, stdio: bool) -> Result<Vec<ProcessHandle>> {
    match from {
        Receive::RtpUdpListener { video, audio_port } => {
            receive_rtp_udp(video.as_ref(), audio_port.is_some(), stdio)
        }
        Receive::RtpTcpClient {
            ip,
            video,
            audio_port,
        } => receive_rtp_tcp(ip, video.as_ref(), audio_port.is_some(), stdio),
        Receive::RtmpListener { .. } => Err(anyhow!(
            "GStreamer backend doesn't support RTMP receive; use Backend::Ffmpeg"
        )),
        Receive::HlsPlayer { .. } => Err(anyhow!(
            "GStreamer backend doesn't support HLS receive; use Backend::Ffmpeg"
        )),
    }
}

fn stdio_for(stdio: bool) -> (Stdio, Stdio) {
    if stdio {
        (Stdio::inherit(), Stdio::inherit())
    } else {
        (Stdio::null(), Stdio::null())
    }
}

#[derive(Copy, Clone)]
enum RtpTransport {
    Udp,
    Tcp,
}

// ---------------------------------------------------------------------------
// Send
// ---------------------------------------------------------------------------

fn send_rtp(
    asset: &ResolvedAsset,
    ip: &str,
    video_port: Option<u16>,
    audio_port: Option<u16>,
    transport: RtpTransport,
    stdio: bool,
) -> Result<Vec<ProcessHandle>> {
    if video_port.is_none() && audio_port.is_none() {
        return Err(anyhow!("At least one of video_port/audio_port must be set"));
    }

    let pipeline = match &asset.kind {
        ResolvedKind::File(path) => {
            build_file_pipeline(path, asset.video, video_port, audio_port, ip, transport)?
        }
        ResolvedKind::Pattern { video, .. } => {
            build_testsrc_pipeline(*video, video_port, audio_port, ip, transport)
        }
    };

    info!("[media] gstreamer: spawning send pipeline");
    let (out, err) = stdio_for(stdio);
    let child = Command::new("bash")
        .arg("-c")
        .arg(pipeline)
        .stdout(out)
        .stderr(err)
        .spawn()?;
    Ok(vec![ProcessHandle::new(child)])
}

fn build_file_pipeline(
    path: &Path,
    codec: Option<VideoCodec>,
    video_port: Option<u16>,
    audio_port: Option<u16>,
    ip: &str,
    transport: RtpTransport,
) -> Result<String> {
    let demuxer = match codec {
        Some(VideoCodec::Vp8 | VideoCodec::Vp9) => "matroskademux",
        _ => "qtdemux",
    };
    let path_str = path.to_string_lossy();
    let mut cmd = format!("gst-launch-1.0 -v filesrc location={path_str} ! {demuxer} name=demux ");

    if let Some(port) = video_port {
        let codec =
            codec.ok_or_else(|| anyhow!("video codec required for file-based gstreamer send"))?;
        let pay = match codec {
            VideoCodec::H264 => "h264parse ! rtph264pay config-interval=1",
            VideoCodec::Vp8 => "rtpvp8pay mtu=1200 picture-id-mode=2",
            VideoCodec::Vp9 => "rtpvp9pay mtu=1200 picture-id-mode=2",
        };
        cmd.push_str(&format!(
            "demux.video_0 ! queue ! {pay} ! application/x-rtp,payload=96 ! {sink} ",
            sink = rtp_sink(ip, port, transport),
        ));
    }
    if let Some(port) = audio_port {
        cmd.push_str(&format!(
            "demux.audio_0 ! queue ! decodebin ! audioconvert ! audioresample ! opusenc ! rtpopuspay ! application/x-rtp,payload=97 ! {sink} ",
            sink = rtp_sink(ip, port, transport),
        ));
    }
    Ok(cmd)
}

fn build_testsrc_pipeline(
    codec: VideoCodec,
    video_port: Option<u16>,
    audio_port: Option<u16>,
    ip: &str,
    transport: RtpTransport,
) -> String {
    let mut cmd = String::from(
        "gst-launch-1.0 -v videotestsrc ! \"video/x-raw,format=I420,width=1920,height=1080,framerate=60/1\" ! ",
    );

    if let Some(port) = video_port {
        let enc = match codec {
            VideoCodec::H264 => "x264enc tune=zerolatency speed-preset=superfast ! rtph264pay",
            VideoCodec::Vp8 => {
                "vp8enc deadline=1 error-resilient=partitions keyframe-max-dist=30 auto-alt-ref=true cpu-used=-5 ! rtpvp8pay"
            }
            VideoCodec::Vp9 => "vp9enc deadline=1 auto-alt-ref=true cpu-used=-5 ! rtpvp9pay",
        };
        cmd.push_str(&format!(
            "{enc} ! application/x-rtp,payload=96 ! {sink} ",
            sink = rtp_sink(ip, port, transport),
        ));
    }
    if let Some(port) = audio_port {
        cmd.push_str(&format!(
            "audiotestsrc ! audioconvert ! audioresample ! opusenc ! rtpopuspay ! application/x-rtp,payload=97 ! {sink} ",
            sink = rtp_sink(ip, port, transport),
        ));
    }
    cmd
}

fn rtp_sink(ip: &str, port: u16, transport: RtpTransport) -> String {
    match transport {
        RtpTransport::Udp => format!("udpsink host={ip} port={port}"),
        RtpTransport::Tcp => format!("rtpstreampay ! tcpclientsink host={ip} port={port}"),
    }
}

// ---------------------------------------------------------------------------
// Receive
// ---------------------------------------------------------------------------

fn receive_rtp_udp(
    video: Option<&RtpVideo>,
    audio: bool,
    stdio: bool,
) -> Result<Vec<ProcessHandle>> {
    if video.is_none() && !audio {
        return Err(anyhow!(
            "At least one of: video, audio_port has to be specified."
        ));
    }

    // Legacy behavior: single udpsrc feeds the demux (video and audio must share a port).
    let port = video
        .map(|v| v.port)
        .ok_or_else(|| anyhow!("gstreamer UDP receive currently requires video port"))?;
    let mut cmd = format!(
        "gst-launch-1.0 -v rtpptdemux name=demux udpsrc port={port} ! \"application/x-rtp\" ! queue ! demux. "
    );
    if let Some(v) = video {
        let (depay, name) = match v.codec {
            VideoCodec::H264 => ("rtph264depay", "H264"),
            VideoCodec::Vp8 => ("rtpvp8depay", "VP8"),
            VideoCodec::Vp9 => ("rtpvp9depay", "VP9"),
        };
        cmd.push_str(&format!(
            "demux.src_96 ! \"application/x-rtp,media=video,clock-rate=90000,encoding-name={name}\" ! queue ! {depay} ! decodebin ! videoconvert ! autovideosink "
        ));
    }
    if audio {
        cmd.push_str(
            "demux.src_97 ! \"application/x-rtp,media=audio,clock-rate=48000,encoding-name=OPUS\" ! queue ! rtpopusdepay ! decodebin ! audioconvert ! autoaudiosink sync=false ",
        );
    }

    info!("[media] gstreamer: receive UDP on {port}");
    let (out, err) = stdio_for(stdio);
    let child = Command::new("bash")
        .arg("-c")
        .arg(cmd)
        .stdout(out)
        .stderr(err)
        .spawn()?;
    thread::sleep(Duration::from_secs(2));
    Ok(vec![ProcessHandle::new(child)])
}

fn receive_rtp_tcp(
    ip: &str,
    video: Option<&RtpVideo>,
    audio: bool,
    stdio: bool,
) -> Result<Vec<ProcessHandle>> {
    let port = video
        .map(|v| v.port)
        .ok_or_else(|| anyhow!("gstreamer TCP receive requires video port"))?;

    if !audio && video.is_none() {
        return Err(anyhow!(
            "At least one of: video, audio has to be specified."
        ));
    }

    let mut cmd = format!(
        "gst-launch-1.0 -v rtpptdemux name=demux tcpclientsrc host={ip} port={port} ! \"application/x-rtp-stream\" ! rtpstreamdepay ! queue ! demux. "
    );
    if let Some(v) = video {
        let (depay, name) = match v.codec {
            VideoCodec::H264 => ("rtph264depay", "H264"),
            VideoCodec::Vp8 => ("rtpvp8depay", "VP8"),
            VideoCodec::Vp9 => ("rtpvp9depay", "VP9"),
        };
        cmd.push_str(&format!(
            "demux.src_96 ! \"application/x-rtp,media=video,clock-rate=90000,encoding-name={name}\" ! queue ! {depay} ! decodebin ! videoconvert ! autovideosink "
        ));
    }
    if audio {
        cmd.push_str(
            "demux.src_97 ! \"application/x-rtp,media=audio,clock-rate=48000,encoding-name=OPUS\" ! queue ! rtpopusdepay ! decodebin ! audioconvert ! autoaudiosink ",
        );
    }

    info!("[media] gstreamer: receive TCP from {ip}:{port}");
    let (out, err) = stdio_for(stdio);
    let child = Command::new("bash")
        .arg("-c")
        .arg(cmd)
        .stdout(out)
        .stderr(err)
        .spawn()?;
    thread::sleep(Duration::from_secs(2));
    Ok(vec![ProcessHandle::new(child)])
}
