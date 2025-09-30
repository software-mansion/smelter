use std::{path::PathBuf, process::Command};

fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    if args.len() != 3 {
        eprintln!(
            "Usage: {} <video/audio-opus/audio-aac/muxed_video_audio> <input_file>",
            args[0]
        );
        return;
    }

    let input_path = PathBuf::from(&args[2]);
    let output_path = input_path
        .parent()
        .unwrap()
        .join(input_path.file_stem().unwrap());

    let input_path = input_path.to_str().unwrap();
    let output_path = output_path.to_str().unwrap();

    let cmd = match args[1].as_str() {
        "video" => {
            format!(
                "gst-launch-1.0 -v funnel name=fn filesrc location={input_path} ! qtdemux ! h264parse ! rtph264pay config-interval=1 pt=96  ! .send_rtp_sink rtpsession name=session .send_rtp_src ! fn. session.send_rtcp_src ! fn. fn. ! rtpstreampay ! filesink location={output_path}_video.rtp"
            )
        }
        "audio-opus" => {
            format!(
                "gst-launch-1.0 -v filesrc location={input_path} ! qtdemux ! rtpopuspay ! application/x-rtp,payload=97 ! rtpstreampay ! filesink location={output_path}_audio.rtp"
            )
        }
        "audio-aac" => {
            format!(
                "gst-launch-1.0 -v filesrc location={input_path} ! qtdemux ! rtpmp4gpay ! application/x-rtp,payload=97 ! rtpstreampay ! filesink location={output_path}_audio_aac.rtp"
            )
        }
        "muxed_video_audio" => {
            format!(
                r#"gst-launch-1.0 -v \
                funnel name=fn ! rtpstreampay ! filesink location={output_path}_video_audio.rtp \
                filesrc location={input_path} ! qtdemux name=demux \
                demux.video_0 ! h264parse ! rtph264pay config-interval=1 pt=96 ! application/x-rtp,payload=96 ! fn.sink_0 \
                demux.audio_0 ! rtpopuspay ! application/x-rtp,payload=97 ! fn.sink_1 \
            "#
            )
        }
        option => panic!("Invalid option \"{option}\". Must be video, audio or muxed_video_audio."),
    };

    Command::new("bash").arg("-c").arg(cmd).status().unwrap();
}
