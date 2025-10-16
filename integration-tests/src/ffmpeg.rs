use anyhow::{Result, anyhow};
use smelter_api::Resolution;
use std::process::Child;
use tracing::info;

use super::examples::{TestSample, get_asset_path};
use std::{
    fs::File,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::Duration,
};

pub enum Video {
    H264,
    VP8,
    VP9,
}

pub fn start_ffmpeg_rtmp_receive(port: u16) -> Result<Child> {
    let output_address = format!("rtmp://0.0.0.0:{port}");

    let handle = Command::new("bash")
        .arg("-c")
        .arg(format!("ffmpeg -f flv -listen 1 -i {output_address} -vcodec copy -f flv - | ffplay -autoexit -f flv -i -"))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    info!("Started RTMP FFmpeg listener on port {port}.");
    thread::sleep(Duration::from_secs(2));

    Ok(handle)
}

pub fn start_ffmpeg_receive_h264(
    video_port: Option<u16>,
    audio_port: Option<u16>,
) -> Result<Child> {
    let output_sdp_path = match (video_port, audio_port) {
        (Some(video_port), Some(audio_port)) => {
            info!(
                "[example] Start listening video on port {video_port} and audio on {audio_port}."
            );
            write_video_audio_example_sdp_file_h264(video_port, audio_port)
        }
        (Some(video_port), None) => {
            info!("[example] Start listening video on port {video_port}.");
            write_video_example_sdp_file_h264(video_port)
        }
        (None, Some(audio_port)) => {
            info!("[example] Start listening audio on {audio_port}.");
            write_audio_example_sdp_file(audio_port)
        }
        (None, None) => {
            return Err(anyhow!(
                "At least one of: 'video_port', 'audio_port' has to be specified."
            ));
        }
    }?;

    let handle = Command::new("ffplay")
        .args(["-protocol_whitelist", "file,rtp,udp", &output_sdp_path])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    thread::sleep(Duration::from_secs(2));

    Ok(handle)
}

pub fn start_ffmpeg_receive_vp8(video_port: Option<u16>, audio_port: Option<u16>) -> Result<Child> {
    let output_sdp_path = match (video_port, audio_port) {
        (Some(video_port), Some(audio_port)) => {
            info!(
                "[example] Start listening video on port {video_port} and audio on {audio_port}."
            );
            write_video_audio_example_sdp_file_vp8(video_port, audio_port)
        }
        (Some(video_port), None) => {
            info!("[example] Start listening video on port {video_port}.");
            write_video_example_sdp_file_vp8(video_port)
        }
        (None, Some(audio_port)) => {
            info!("[example] Start listening audio on {audio_port}.");
            write_audio_example_sdp_file(audio_port)
        }
        (None, None) => {
            return Err(anyhow!(
                "At least one of: 'video_port', 'audio_port' has to be specified."
            ));
        }
    }?;

    let handle = Command::new("ffplay")
        .args(["-protocol_whitelist", "file,rtp,udp", &output_sdp_path])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    thread::sleep(Duration::from_secs(2));

    Ok(handle)
}

pub fn start_ffmpeg_receive_vp9(video_port: Option<u16>, audio_port: Option<u16>) -> Result<Child> {
    let output_sdp_path = match (video_port, audio_port) {
        (Some(video_port), Some(audio_port)) => {
            info!(
                "[example] Start listening video on port {video_port} and audio on {audio_port}."
            );
            write_video_audio_example_sdp_file_vp9(video_port, audio_port)
        }
        (Some(video_port), None) => {
            info!("[example] Start listening video on port {video_port}.");
            write_video_example_sdp_file_vp9(video_port)
        }
        (None, Some(audio_port)) => {
            info!("[example] Start listening audio on {audio_port}.");
            write_audio_example_sdp_file(audio_port)
        }
        (None, None) => {
            return Err(anyhow!(
                "At least one of: 'video_port', 'audio_port' has to be specified."
            ));
        }
    }?;

    let handle = Command::new("ffplay")
        .args(["-protocol_whitelist", "file,rtp,udp", &output_sdp_path])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    thread::sleep(Duration::from_secs(2));

    Ok(handle)
}

pub fn start_ffmpeg_receive_hls(playlist_path: &Path) -> Result<Child> {
    for _ in 0..20 {
        if playlist_path.exists() && !std::fs::read_to_string(playlist_path)?.trim().is_empty() {
            break;
        }
        thread::sleep(Duration::from_secs(1));
    }
    if !playlist_path.exists() || std::fs::read_to_string(playlist_path)?.trim().is_empty() {
        return Err(anyhow!("Playlist file does not exist: {playlist_path:?}"));
    }

    let handle = Command::new("ffplay")
        .args(["-i", playlist_path.to_str().unwrap()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    thread::sleep(Duration::from_secs(2));

    Ok(handle)
}

pub fn start_ffmpeg_send(
    ip: &str,
    video_port: Option<u16>,
    audio_port: Option<u16>,
    test_sample: TestSample,
) -> Result<(Option<Child>, Option<Child>)> {
    match test_sample {
        TestSample::BigBuckBunnyH264Opus | TestSample::ElephantsDreamH264Opus => {
            start_ffmpeg_send_from_file(
                ip,
                video_port,
                audio_port,
                get_asset_path(test_sample)?,
                Some(Video::H264),
            )
        }
        TestSample::BigBuckBunnyVP8Opus | TestSample::ElephantsDreamVP8Opus => {
            start_ffmpeg_send_from_file(
                ip,
                video_port,
                audio_port,
                get_asset_path(test_sample)?,
                Some(Video::VP8),
            )
        }
        TestSample::BigBuckBunnyVP9Opus | TestSample::ElephantsDreamVP9Opus => {
            start_ffmpeg_send_from_file(
                ip,
                video_port,
                audio_port,
                get_asset_path(test_sample)?,
                Some(Video::VP9),
            )
        }
        TestSample::BigBuckBunnyH264AAC => start_ffmpeg_send_from_file_aac(
            ip,
            video_port,
            audio_port,
            get_asset_path(test_sample)?,
            Video::H264,
        ),
        TestSample::SampleH264 => match video_port {
            Some(port) => start_ffmpeg_send_from_file(
                ip,
                Some(port),
                None,
                get_asset_path(test_sample)?,
                Some(Video::H264),
            ),
            None => Err(anyhow!("video port required for test sample")),
        },
        TestSample::SampleVP8 => match video_port {
            Some(port) => start_ffmpeg_send_from_file(
                ip,
                Some(port),
                None,
                get_asset_path(test_sample)?,
                Some(Video::VP8),
            ),
            None => Err(anyhow!("video port required for test sample")),
        },
        TestSample::SampleVP9 => match video_port {
            Some(port) => start_ffmpeg_send_from_file(
                ip,
                Some(port),
                None,
                get_asset_path(test_sample)?,
                Some(Video::VP9),
            ),
            None => Err(anyhow!("video port required for test sample")),
        },
        TestSample::SampleLoopH264 => match video_port {
            Some(port) => Ok((
                Some(start_ffmpeg_send_video_from_file_loop(
                    ip,
                    port,
                    get_asset_path(test_sample)?,
                )?),
                None,
            )),
            None => Err(anyhow!("video port required for test sample")),
        },
        TestSample::TestPatternH264 => match video_port {
            Some(port) => Ok((
                Some(start_ffmpeg_send_testsrc(
                    ip,
                    port,
                    Resolution {
                        width: 1920,
                        height: 1080,
                    },
                    Video::H264,
                )?),
                None,
            )),
            None => Err(anyhow!("video port required for generic")),
        },
        TestSample::TestPatternVP8 => match video_port {
            Some(port) => Ok((
                Some(start_ffmpeg_send_testsrc(
                    ip,
                    port,
                    Resolution {
                        width: 1280,
                        height: 720,
                    },
                    Video::VP8,
                )?),
                None,
            )),
            None => Err(anyhow!("video port required for generic")),
        },
        TestSample::TestPatternVP9 => match video_port {
            Some(port) => Ok((
                Some(start_ffmpeg_send_testsrc(
                    ip,
                    port,
                    Resolution {
                        width: 1920,
                        height: 1080,
                    },
                    Video::VP9,
                )?),
                None,
            )),
            None => Err(anyhow!("video port required for generic")),
        },
    }
}

pub fn start_ffmpeg_send_from_file(
    ip: &str,
    video_port: Option<u16>,
    audio_port: Option<u16>,
    path: PathBuf,
    video_codec: Option<Video>,
) -> Result<(Option<Child>, Option<Child>)> {
    if video_port.is_none() && audio_port.is_none() {
        return Err(anyhow!(
            "At least one of: 'video_port', 'audio_port' has to be specified."
        ));
    }

    let video_handle = match video_port {
        Some(port) => Some(start_ffmpeg_send_video_from_file(
            ip,
            port,
            path.clone(),
            video_codec.unwrap(),
        )?),
        None => None,
    };

    let audio_handle = match audio_port {
        Some(port) => Some(start_ffmpeg_send_audio_from_file(
            ip, port, path, "libopus",
        )?),
        None => None,
    };

    Ok((video_handle, audio_handle))
}

fn start_ffmpeg_send_from_file_aac(
    ip: &str,
    video_port: Option<u16>,
    audio_port: Option<u16>,
    path: PathBuf,
    video_codec: Video,
) -> Result<(Option<Child>, Option<Child>)> {
    if video_port.is_none() && audio_port.is_none() {
        return Err(anyhow!(
            "At least one of: 'video_port', 'audio_port' has to be specified."
        ));
    }

    let video_handle = match video_port {
        Some(port) => Some(start_ffmpeg_send_video_from_file(
            ip,
            port,
            path.clone(),
            video_codec,
        )?),
        None => None,
    };

    let audio_handle = match audio_port {
        Some(port) => Some(start_ffmpeg_send_audio_from_file(
            ip, port, path, "libopus",
        )?),
        None => None,
    };

    Ok((video_handle, audio_handle))
}

fn start_ffmpeg_send_video_from_file(
    ip: &str,
    port: u16,
    path: PathBuf,
    video_codec: Video,
) -> Result<Child> {
    info!("[example] Start sending video to input port {port}.");

    let codec_specific_options = match video_codec {
        Video::H264 => vec!["-bsf:v", "h264_mp4toannexb"],
        Video::VP8 => vec![],
        Video::VP9 => vec!["-strict", "experimental"],
    };

    let handle = Command::new("ffmpeg")
        .args(["-re", "-i"])
        .arg(path)
        .args(["-an", "-c:v", "copy", "-f", "rtp"])
        .args(codec_specific_options)
        .arg(format!("rtp://{ip}:{port}?rtcpport={port}"))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    Ok(handle)
}

fn start_ffmpeg_send_video_from_file_loop(ip: &str, port: u16, path: PathBuf) -> Result<Child> {
    info!("[example] Start sending video loop to input port {port}.");

    let handle = Command::new("ffmpeg")
        .args(["-stream_loop", "-1", "-re", "-i"])
        .arg(path)
        .args([
            "-an",
            "-c:v",
            "copy",
            "-f",
            "rtp",
            "-bsf:v",
            "h264_mp4toannexb",
            &format!("rtp://{ip}:{port}?rtcpport={port}"),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    Ok(handle)
}

fn start_ffmpeg_send_audio_from_file(
    ip: &str,
    port: u16,
    path: PathBuf,
    codec: &str,
) -> Result<Child> {
    info!("[example] Start sending audio to input port {port}.");

    let handle = Command::new("ffmpeg")
        .args(["-stream_loop", "-1", "-re", "-i"])
        .arg(path.clone())
        .args([
            "-vn",
            "-c:a",
            codec,
            "-f",
            "rtp",
            &format!("rtp://{ip}:{port}?rtcpport={port}"),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    Ok(handle)
}

fn start_ffmpeg_send_testsrc(
    ip: &str,
    port: u16,
    resolution: Resolution,
    video_codec: Video,
) -> Result<Child> {
    info!("[example] Start sending generic video to input port {port}.");

    let ffmpeg_source = format!(
        "testsrc=s={}x{}:r=30,format=yuv420p",
        resolution.width, resolution.height
    );

    let codec = match video_codec {
        Video::H264 => vec!["libx264"],
        Video::VP8 => vec![
            "libvpx",
            "-deadline",
            "realtime",
            "-error-resilient",
            "1",
            "-b:v",
            "1M",
        ],
        Video::VP9 => vec![
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

    let handle = Command::new("ffmpeg")
        .args(["-re", "-f", "lavfi", "-i", &ffmpeg_source, "-c:v"])
        .args(codec)
        .args(["-f", "rtp", &format!("rtp://{ip}:{port}?rtcpport={port}")])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    Ok(handle)
}

/// The SDP file will describe an RTP session on localhost with H264 encoding.
fn write_video_example_sdp_file_h264(port: u16) -> Result<String> {
    let ip = "127.0.0.1";
    let sdp_filepath = PathBuf::from(format!("/tmp/example_sdp_video_input_{port}.sdp"));
    let mut file = File::create(&sdp_filepath)?;
    file.write_all(
        format!(
            "\
                    v=0\n\
                    o=- 0 0 IN IP4 {ip}\n\
                    s=No Name\n\
                    c=IN IP4 {ip}\n\
                    m=video {port} RTP/AVP 96\n\
                    a=rtpmap:96 H264/90000\n\
                    a=fmtp:96 packetization-mode=1\n\
                    a=rtcp-mux\n\
                "
        )
        .as_bytes(),
    )?;
    Ok(String::from(
        sdp_filepath
            .to_str()
            .ok_or_else(|| anyhow!("invalid utf string"))?,
    ))
}

fn write_video_example_sdp_file_vp8(port: u16) -> Result<String> {
    let ip = "127.0.0.1";
    let sdp_filepath = PathBuf::from(format!("/tmp/example_sdp_video_input_{port}.sdp"));
    let mut file = File::create(&sdp_filepath)?;
    file.write_all(
        format!(
            "\
                    v=0\n\
                    o=- 0 0 IN IP4 {ip}\n\
                    s=No Name\n\
                    c=IN IP4 {ip}\n\
                    m=video {port} RTP/AVP 96\n\
                    a=rtpmap:96 VP8/90000\n\
                    a=rtcp-mux\n\
                "
        )
        .as_bytes(),
    )?;
    Ok(String::from(
        sdp_filepath
            .to_str()
            .ok_or_else(|| anyhow!("invalid utf string"))?,
    ))
}

fn write_video_example_sdp_file_vp9(port: u16) -> Result<String> {
    let ip = "127.0.0.1";
    let sdp_filepath = PathBuf::from(format!("/tmp/example_sdp_video_input_{port}.sdp"));
    let mut file = File::create(&sdp_filepath)?;
    file.write_all(
        format!(
            "\
                    v=0\n\
                    o=- 0 0 IN IP4 {ip}\n\
                    s=No Name\n\
                    c=IN IP4 {ip}\n\
                    m=video {port} RTP/AVP 96\n\
                    a=rtpmap:96 VP9/90000\n\
                    a=rtcp-mux\n\
                "
        )
        .as_bytes(),
    )?;
    Ok(String::from(
        sdp_filepath
            .to_str()
            .ok_or_else(|| anyhow!("invalid utf string"))?,
    ))
}

/// The SDP file will describe an RTP session on localhost with H264 video encoding and Opus audio encoding.
fn write_video_audio_example_sdp_file_h264(video_port: u16, audio_port: u16) -> Result<String> {
    let ip = "127.0.0.1";
    let sdp_filepath = PathBuf::from(format!(
        "/tmp/example_sdp_video_audio_input_{video_port}.sdp"
    ));
    let mut file = File::create(&sdp_filepath)?;
    file.write_all(
        format!(
            "\
                    v=0\n\
                    o=- 0 0 IN IP4 {ip}\n\
                    s=No Name\n\
                    c=IN IP4 {ip}\n\
                    m=video {video_port} RTP/AVP 96\n\
                    a=rtpmap:96 H264/90000\n\
                    a=fmtp:96 packetization-mode=1\n\
                    a=rtcp-mux\n\
                    m=audio {audio_port} RTP/AVP 97\n\
                    a=rtpmap:97 opus/48000/2\n\
                "
        )
        .as_bytes(),
    )?;
    Ok(String::from(
        sdp_filepath
            .to_str()
            .ok_or_else(|| anyhow!("invalid utf string"))?,
    ))
}

fn write_video_audio_example_sdp_file_vp8(video_port: u16, audio_port: u16) -> Result<String> {
    let ip = "127.0.0.1";
    let sdp_filepath = PathBuf::from(format!(
        "/tmp/example_sdp_video_audio_input_{video_port}.sdp"
    ));
    let mut file = File::create(&sdp_filepath)?;
    file.write_all(
        format!(
            "\
                    v=0\n\
                    o=- 0 0 IN IP4 {ip}\n\
                    s=No Name\n\
                    c=IN IP4 {ip}\n\
                    m=video {video_port} RTP/AVP 96\n\
                    a=rtpmap:96 VP8/90000\n\
                    a=rtcp-mux\n\
                    m=audio {audio_port} RTP/AVP 97\n\
                    a=rtpmap:97 opus/48000/2\n\
                "
        )
        .as_bytes(),
    )?;
    Ok(String::from(
        sdp_filepath
            .to_str()
            .ok_or_else(|| anyhow!("invalid utf string"))?,
    ))
}

fn write_video_audio_example_sdp_file_vp9(video_port: u16, audio_port: u16) -> Result<String> {
    let ip = "127.0.0.1";
    let sdp_filepath = PathBuf::from(format!(
        "/tmp/example_sdp_video_audio_input_{video_port}.sdp"
    ));
    let mut file = File::create(&sdp_filepath)?;
    file.write_all(
        format!(
            "\
                    v=0\n\
                    o=- 0 0 IN IP4 {ip}\n\
                    s=No Name\n\
                    c=IN IP4 {ip}\n\
                    m=video {video_port} RTP/AVP 96\n\
                    a=rtpmap:96 VP9/90000\n\
                    a=rtcp-mux\n\
                    m=audio {audio_port} RTP/AVP 97\n\
                    a=rtpmap:97 opus/48000/2\n\
                "
        )
        .as_bytes(),
    )?;
    Ok(String::from(
        sdp_filepath
            .to_str()
            .ok_or_else(|| anyhow!("Invalid UTF string"))?,
    ))
}

/// The SDP file will describe an RTP session on localhost with Opus audio encoding.
fn write_audio_example_sdp_file(port: u16) -> Result<String> {
    let ip = "127.0.0.1";
    let sdp_filepath = PathBuf::from(format!("/tmp/example_sdp_audio_input_{port}.sdp"));
    let mut file = File::create(&sdp_filepath)?;
    file.write_all(
        format!(
            "\
                    v=0\n\
                    o=- 0 0 IN IP4 {ip}\n\
                    s=No Name\n\
                    c=IN IP4 {ip}\n\
                    m=audio {port} RTP/AVP 97\n\
                    a=rtpmap:97 opus/48000/2\n\
                "
        )
        .as_bytes(),
    )?;
    Ok(String::from(
        sdp_filepath
            .to_str()
            .ok_or_else(|| anyhow!("invalid utf string"))?,
    ))
}
