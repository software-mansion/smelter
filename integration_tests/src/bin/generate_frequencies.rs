use std::process::Command;

use integration_tests::paths::submodule_root_path;

enum Encoder {
    Aac,
    Opus,
}

impl Encoder {
    fn name(&self) -> String {
        match self {
            Self::Aac => "aac".to_string(),
            Self::Opus => "libopus".to_string(),
        }
    }

    fn file_name(&self) -> String {
        match self {
            Self::Aac => "aac".to_string(),
            Self::Opus => "opus".to_string(),
        }
    }
}

fn main() {
    let encoders = vec![Encoder::Aac, Encoder::Opus];
    let notes = vec![("a", 440.0), ("c_sharp", 554.37), ("e", 659.26)];

    let cmd_wd = submodule_root_path()
        .join("rtp_packet_dumps")
        .join("inputs");

    for encoder in &encoders {
        for (name, freq) in &notes {
            let file_suffix = encoder.file_name();
            let encoder_name = encoder.name();

            let file_name = format!("{name}_{file_suffix}.mp4");

            let cmd = format!("ffmpeg -y -f lavfi -i \"sine=frequency={freq}:sample_rate=48000:duration=20\" -af \"volume=3\" -c:a {encoder_name} -b:a 192k -ac 2 -ar 48000 -vn {file_name}");

            Command::new("bash")
                .arg("-c")
                .arg(cmd)
                .current_dir(&cmd_wd)
                .status()
                .unwrap();

            Command::new("bash")
                .arg("-c")
                .arg(format!(
                    "cargo run -p integration_tests --bin generate_rtp_from_file audio-{file_suffix} {file_name}"
                ))
                .current_dir(&cmd_wd)
                .status()
                .unwrap();
        }
    }
}
