use std::process::Command;

fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    if args.len() != 3 {
        eprintln!("Usage: {} <video/audio/av> <input_file>", args[0]);
        return;
    }

    let input_file = &args[2];
    let command = match args[1].as_str() {
        "video" => {
            format!(
                "gst-launch-1.0 -v filesrc location={input_file} ! application/x-rtp-stream ! rtpstreamdepay ! rtph264depay ! video/x-h264,framerate=30/1 ! h264parse ! h264timestamper ! decodebin ! videoconvert ! autovideosink"
            )
        }
        "audio" => {
            format!(
                "gst-launch-1.0 -v filesrc location={input_file} ! application/x-rtp-stream,payload=97,encoding-name=OPUS ! rtpstreamdepay ! rtpopusdepay ! audio/x-opus ! opusdec ! autoaudiosink"
            )
        }
        "av" => {
            [
                "gst-launch-1.0 rtpptdemux name=demux ",
                &format!("filesrc location={input_file} ! \"application/x-rtp-stream\" ! rtpstreamdepay ! queue ! demux. "),
                "demux.src_96 ! \"application/x-rtp,media=video,clock-rate=90000,encoding-name=H264\" ! queue ",
                "! rtph264depay ! decodebin ! videoconvert ! autovideosink ",
                "demux.src_97 ! \"application/x-rtp,media=audio,clock-rate=48000,encoding-name=OPUS\" ! queue ",
                "! rtpopusdepay ! decodebin ! audioconvert ! autoaudiosink  ",
            ].concat()
        }
        option => panic!("Invalid option \"{option}\". Must be video or audio."),
    };

    Command::new("bash")
        .arg("-c")
        .arg(command)
        .status()
        .unwrap();
}
