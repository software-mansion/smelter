use anyhow::Result;
use std::{fs::File, io::Write, path::PathBuf};

use super::VideoCodec;

pub(super) fn write_sdp(
    video: Option<(u16, VideoCodec)>,
    audio_port: Option<u16>,
) -> Result<PathBuf> {
    let ip = "127.0.0.1";
    let tag = match (video.as_ref().map(|(p, _)| *p), audio_port) {
        (Some(vp), Some(ap)) => format!("va_{vp}_{ap}"),
        (Some(vp), None) => format!("v_{vp}"),
        (None, Some(ap)) => format!("a_{ap}"),
        (None, None) => "empty".to_string(),
    };
    let path = PathBuf::from(format!("/tmp/smelter_sdp_{tag}.sdp"));

    let mut body = String::from(
        "v=0\n\
         o=- 0 0 IN IP4 127.0.0.1\n\
         s=No Name\n\
         c=IN IP4 127.0.0.1\n",
    );
    let _ = ip;

    if let Some((port, codec)) = video {
        match codec {
            VideoCodec::H264 => body.push_str(&format!(
                "m=video {port} RTP/AVP 96\n\
                 a=rtpmap:96 H264/90000\n\
                 a=fmtp:96 packetization-mode=1\n\
                 a=rtcp-mux\n"
            )),
            VideoCodec::Vp8 => body.push_str(&format!(
                "m=video {port} RTP/AVP 96\n\
                 a=rtpmap:96 VP8/90000\n\
                 a=rtcp-mux\n"
            )),
            VideoCodec::Vp9 => body.push_str(&format!(
                "m=video {port} RTP/AVP 96\n\
                 a=rtpmap:96 VP9/90000\n\
                 a=rtcp-mux\n"
            )),
        }
    }
    if let Some(port) = audio_port {
        body.push_str(&format!(
            "m=audio {port} RTP/AVP 97\n\
             a=rtpmap:97 opus/48000/2\n"
        ));
    }

    let mut file = File::create(&path)?;
    file.write_all(body.as_bytes())?;
    Ok(path)
}
