use anyhow::{anyhow, Result};
use log::info;

use std::{
    path::PathBuf,
    process::{Command, Stdio},
    thread,
    time::Duration,
};

use super::examples::{get_asset_path, TestSample};

#[derive(Clone)]
enum Video {
    H264,
    VP8,
}

pub fn start_gst_receive_tcp_h264(ip: &str, port: u16, audio: bool) -> Result<()> {
    start_gst_receive_tcp(ip, port, Some(Video::H264), audio)?;
    Ok(())
}

pub fn start_gst_receive_tcp_vp8(ip: &str, port: u16, audio: bool) -> Result<()> {
    start_gst_receive_tcp(ip, port, Some(Video::VP8), audio)?;
    Ok(())
}

pub fn start_gst_receive_tcp_without_video(ip: &str, port: u16, audio: bool) -> Result<()> {
    start_gst_receive_tcp(ip, port, None, audio)?;
    Ok(())
}

fn start_gst_receive_tcp(ip: &str, port: u16, video: Option<Video>, audio: bool) -> Result<()> {
    match (video.clone(), audio) {
        (Some(_), true) => info!("[example] Start listening video and audio on port {port}."),
        (Some(_), false) => info!("[example] Start listening video on port {port}."),
        (None, true) => info!("[example] Start listening audio on port {port}."),
        (None, false) => return Err(anyhow!("At least one of: 'video', 'audio' has to be true.")),
    }

    let mut gst_output_command = [
        "gst-launch-1.0 -v ",
        "rtpptdemux name=demux ",
        &format!("tcpclientsrc host={} port={} ! \"application/x-rtp-stream\" ! rtpstreamdepay ! queue ! demux. ", ip, port)
        ].concat();

    match video {
        Some(Video::H264) => gst_output_command.push_str("demux.src_96 ! \"application/x-rtp,media=video,clock-rate=90000,encoding-name=H264\" ! queue ! rtph264depay ! decodebin ! videoconvert ! autovideosink "),
        Some(Video::VP8) => gst_output_command.push_str("demux.src_96 ! \"application/x-rtp,media=video,clock-rate=90000,encoding-name=VP8\" ! queue ! rtpvp8depay ! decodebin ! videoconvert ! autovideosink "),
        None => {}
    }
    if audio {
        gst_output_command.push_str("demux.src_97 ! \"application/x-rtp,media=audio,clock-rate=48000,encoding-name=OPUS\" ! queue ! rtpopusdepay ! decodebin ! audioconvert ! autoaudiosink ");
    }

    Command::new("bash")
        .arg("-c")
        .arg(gst_output_command)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    thread::sleep(Duration::from_secs(2));

    Ok(())
}

pub fn start_gst_receive_udp_h264(port: u16, audio: bool) -> Result<()> {
    start_gst_receive_udp(port, Some(Video::H264), audio)
}

pub fn start_gst_receive_udp_vp8(port: u16, audio: bool) -> Result<()> {
    start_gst_receive_udp(port, Some(Video::VP8), audio)
}

pub fn start_gst_receive_udp_without_video(port: u16, audio: bool) -> Result<()> {
    start_gst_receive_udp(port, None, audio)
}

fn start_gst_receive_udp(port: u16, video: Option<Video>, audio: bool) -> Result<()> {
    match (video.clone(), audio) {
        (Some(_), true) => info!("[example] Start listening video and audio on port {port}."),
        (Some(_), false) => info!("[example] Start listening video on port {port}."),
        (None, true) => info!("[example] Start listening audio on port {port}."),
        (None, false) => return Err(anyhow!("At least one of: 'video', 'audio' has to be true.")),
    }

    let mut gst_output_command = [
        "gst-launch-1.0 -v ",
        "rtpptdemux name=demux ",
        &format!(
            "udpsrc port={} ! \"application/x-rtp\" ! queue ! demux. ",
            port
        ),
    ]
    .concat();

    match video {
        Some(Video::H264) => gst_output_command.push_str("demux.src_96 ! \"application/x-rtp,media=video,clock-rate=90000,encoding-name=H264\" ! queue ! rtph264depay ! decodebin ! videoconvert ! autovideosink "),
        Some(Video::VP8) => gst_output_command.push_str("demux.src_96 ! \"application/x-rtp,media=video,clock-rate=90000,encoding-name=VP8\" ! queue ! rtpvp8depay ! decodebin ! videoconvert ! autovideosink "),
        None => {}
    }
    if audio {
        gst_output_command.push_str("demux.src_97 ! \"application/x-rtp,media=audio,clock-rate=48000,encoding-name=OPUS\" ! queue ! rtpopusdepay ! decodebin ! audioconvert ! autoaudiosink sync=false");
    }

    Command::new("bash")
        .arg("-c")
        .arg(gst_output_command)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    thread::sleep(Duration::from_secs(2));

    Ok(())
}

pub fn start_gst_send_tcp(
    ip: &str,
    video_port: Option<u16>,
    audio_port: Option<u16>,
    test_sample: TestSample,
) -> Result<()> {
    match test_sample {
        TestSample::BigBuckBunnyH264Opus
        | TestSample::ElephantsDreamH264Opus
        | TestSample::SampleH264 => start_gst_send_from_file_tcp(
            ip,
            video_port,
            audio_port,
            get_asset_path(test_sample)?,
            Some(Video::H264),
        ),
        TestSample::BigBuckBunnyVP8Opus
        | TestSample::ElephantsDreamVP8Opus
        | TestSample::SampleVP8 => start_gst_send_from_file_tcp(
            ip,
            video_port,
            audio_port,
            get_asset_path(test_sample)?,
            Some(Video::VP8),
        ),
        TestSample::BigBuckBunnyH264AAC => Err(anyhow!(
            "GStreamer does not support AAC, try ffmpeg instead"
        )),
        TestSample::SampleLoopH264 => Err(anyhow!(
            "Cannot play sample in loop using gstreamer, try ffmpeg instead."
        )),
        TestSample::TestPatternH264 => {
            start_gst_send_testsrc_tcp(ip, video_port, audio_port, Some(Video::H264))
        }
        TestSample::TestPatternVP8 => {
            start_gst_send_testsrc_tcp(ip, video_port, audio_port, Some(Video::VP8))
        }
    }
}

pub fn start_gst_send_udp(
    ip: &str,
    video_port: Option<u16>,
    audio_port: Option<u16>,
    test_sample: TestSample,
) -> Result<()> {
    match test_sample {
        TestSample::BigBuckBunnyH264Opus
        | TestSample::ElephantsDreamH264Opus
        | TestSample::SampleH264 => start_gst_send_from_file_udp(
            ip,
            video_port,
            audio_port,
            get_asset_path(test_sample)?,
            Some(Video::H264),
        ),
        TestSample::BigBuckBunnyVP8Opus
        | TestSample::ElephantsDreamVP8Opus
        | TestSample::SampleVP8 => start_gst_send_from_file_udp(
            ip,
            video_port,
            audio_port,
            get_asset_path(test_sample)?,
            Some(Video::VP8),
        ),
        TestSample::BigBuckBunnyH264AAC => Err(anyhow!(
            "GStreamer does not support AAC, try ffmpeg instead"
        )),
        TestSample::SampleLoopH264 => Err(anyhow!(
            "Cannot play sample in loop using gstreamer, try ffmpeg instead."
        )),
        TestSample::TestPatternH264 => {
            start_gst_send_testsrc_udp(ip, video_port, audio_port, Some(Video::H264))
        }
        TestSample::TestPatternVP8 => {
            start_gst_send_testsrc_udp(ip, video_port, audio_port, Some(Video::VP8))
        }
    }
}

fn start_gst_send_from_file_tcp(
    ip: &str,
    video_port: Option<u16>,
    audio_port: Option<u16>,
    path: PathBuf,
    video_codec: Option<Video>,
) -> Result<()> {
    match (video_port, audio_port) {
        (Some(video_port), Some(audio_port)) => info!(
            "[example] Start sending video on port {video_port} and audio on port {audio_port}."
        ),
        (Some(video_port), None) => info!("[example] Start sending video on port {video_port}."),
        (None, Some(audio_port)) => info!("[example] Start sending audio on port {audio_port}."),
        (None, None) => {
            return Err(anyhow!(
                "At least one of: 'video_port', 'audio_port' has to be specified."
            ))
        }
    }

    let demuxer = match video_codec {
        Some(Video::VP8) => "matroskademux",
        _ => "qtdemux",
    };

    let path = path.to_string_lossy();

    let mut gst_input_command =
        format!("gst-launch-1.0 -v filesrc location={path} ! {demuxer} name=demux  ");

    if let (Some(port), Some(codec)) = (video_port, video_codec) {
        let command_video_spec = match codec {
            Video::H264 =>  &format!("demux.video_0 ! queue ! h264parse ! rtph264pay config-interval=1 !  application/x-rtp,payload=96 ! rtpstreampay ! tcpclientsink host={ip} port={port} "),
            Video::VP8 => &format!("demux.video_0 ! queue ! vp8parse ! rtpvp8pay ! application/x-rtp,payload=96 ! rtpstreampay ! tcpclientsink host={ip} port={port}"),
        };
        gst_input_command = gst_input_command + command_video_spec
    }
    if let Some(port) = audio_port {
        gst_input_command = gst_input_command + &format!("demux.audio_0 ! queue ! decodebin ! audioconvert ! audioresample ! opusenc ! rtpopuspay ! application/x-rtp,payload=97 !  rtpstreampay ! tcpclientsink host={ip} port={port} ");
    }

    Command::new("bash")
        .arg("-c")
        .arg(gst_input_command)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    Ok(())
}

fn start_gst_send_from_file_udp(
    ip: &str,
    video_port: Option<u16>,
    audio_port: Option<u16>,
    path: PathBuf,
    video_codec: Option<Video>,
) -> Result<()> {
    match (video_port, audio_port) {
        (Some(video_port), Some(audio_port)) => info!(
            "[example] Start sending video on port {video_port} and audio on port {audio_port}."
        ),
        (Some(video_port), None) => info!("[example] Start sending video on port {video_port}."),
        (None, Some(audio_port)) => info!("[example] Start sending audio on port {audio_port}."),
        (None, None) => {
            return Err(anyhow!(
                "At least one of: 'video_port', 'audio_port' has to be specified."
            ))
        }
    }

    let path = path.to_string_lossy();

    let demuxer = match video_codec {
        Some(Video::VP8) => "matroskademux",
        _ => "qtdemux",
    };

    let mut gst_input_command = [
        "gst-launch-1.0 -v ",
        &format!("filesrc location={path} ! {demuxer} name=demux ",),
    ]
    .concat();

    if let (Some(port), Some(codec)) = (video_port, video_codec) {
        let command_video_spec = match codec {
            Video::H264 =>  &format!(" demux.video_0 ! queue ! h264parse ! rtph264pay config-interval=1 !  application/x-rtp,payload=96  ! udpsink host={ip} port={port} "),
            Video::VP8 => &format!(" demux.video_0 ! queue ! rtpvp8pay !  application/x-rtp,payload=96  ! udpsink host={ip} port={port} "),
        };
        gst_input_command = gst_input_command + command_video_spec
    }
    if let Some(port) = audio_port {
        gst_input_command = gst_input_command + &format!("demux.audio_0 ! queue ! decodebin ! audioconvert ! audioresample ! opusenc ! rtpopuspay ! application/x-rtp,payload=97 ! udpsink host={ip} port={port} ");
    }

    Command::new("bash")
        .arg("-c")
        .arg(gst_input_command)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    Ok(())
}

fn start_gst_send_testsrc_tcp(
    ip: &str,
    video_port: Option<u16>,
    audio_port: Option<u16>,
    video_codec: Option<Video>,
) -> Result<()> {
    match (video_port, audio_port) {
        (Some(video_port), Some(audio_port)) => info!(
            "[example] Start sending generic video on port {video_port} and audio on port {audio_port}."
        ),
        (Some(video_port), None) => info!("[example] Start sending generic video on port {video_port}."),
        (None, Some(audio_port)) => info!("[example] Start sending generic audio on port {audio_port}."),
        (None, None) => {
            return Err(anyhow!(
                "At least one of: 'video_port', 'audio_port' has to be specified."
            ))
        }
    }

    let mut gst_input_command = [
        "gst-launch-1.0 -v videotestsrc ! ",
        "\"video/x-raw,format=I420,width=1920,height=1080,framerate=60/1\" ! ",
    ]
    .concat();

    if let (Some(port), Some(codec)) = (video_port, video_codec) {
        let command_video_spec = match codec {
            Video::H264 =>  &format!(" x264enc tune=zerolatency speed-preset=superfast ! rtph264pay ! application/x-rtp,payload=96 ! rtpstreampay ! tcpclientsink host={ip} port={port}"),
            Video::VP8 => &format!(" vp8enc deadline=1 error-resilient=partitions keyframe-max-dist=30 auto-alt-ref=true cpu-used=-5 ! rtpvp8pay ! application/x-rtp,payload=96 ! rtpstreampay ! tcpclientsink host={ip} port={port}"),
        };
        gst_input_command = gst_input_command + command_video_spec
    }
    if let Some(port) = audio_port {
        gst_input_command = gst_input_command + &format!(" audiotestsrc ! audioconvert ! audioresample ! opusenc ! rtpopuspay ! application/x-rtp,payload=97 ! rtpstreampay ! tcpclientsink host={ip} port={port}");
    }

    Command::new("bash")
        .arg("-c")
        .arg(gst_input_command)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    Ok(())
}

fn start_gst_send_testsrc_udp(
    ip: &str,
    video_port: Option<u16>,
    audio_port: Option<u16>,
    video_codec: Option<Video>,
) -> Result<()> {
    match (video_port, audio_port) {
        (Some(video_port), Some(audio_port)) => info!(
            "[example] Start sending generic video on port {video_port} and audio on port {audio_port}."
        ),
        (Some(video_port), None) => info!("[example] Start sending generic video on port {video_port}."),
        (None, Some(audio_port)) => info!("[example] Start sending generic audio on port {audio_port}."),
        (None, None) => {
            return Err(anyhow!(
                "At least one of: 'video_port', 'audio_port' has to be specified."
            ))
        }
    }

    let mut gst_input_command = [
        "gst-launch-1.0 -v videotestsrc pattern=ball ! ",
        "\"video/x-raw,format=I420,width=1920,height=1080,framerate=60/1\" ! ",
    ]
    .concat();

    if let (Some(port), Some(codec)) = (video_port, video_codec) {
        let command_video_spec = match codec {
            Video::H264 =>  &format!(" x264enc tune=zerolatency speed-preset=superfast ! rtph264pay ! application/x-rtp,payload=96 ! udpsink host={ip} port={port}"),
            Video::VP8 => &format!(" vp8enc deadline=1 error-resilient=partitions keyframe-max-dist=30 auto-alt-ref=true cpu-used=-5 ! rtpvp8pay ! application/x-rtp,payload=96 ! udpsink host={ip} port={port}"),
        };
        gst_input_command = gst_input_command + command_video_spec
    }
    if let Some(port) = audio_port {
        gst_input_command = gst_input_command + &format!(" audiotestsrc ! audioconvert ! audioresample ! opusenc ! rtpopuspay ! application/x-rtp,payload=97 ! udpsink host={ip} port={port}");
    }

    Command::new("bash")
        .arg("-c")
        .arg(gst_input_command)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    Ok(())
}
