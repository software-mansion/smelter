use std::{fs, process::Command};

use integration_tests::paths::{integration_tests_root, submodule_root_path};

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

    let snapshots_dir = submodule_root_path()
        .join("rtp_packet_dumps")
        .join("inputs");

    for encoder in &encoders {
        for (name, freq) in &notes {
            let file_suffix = encoder.file_name();
            let encoder_name = encoder.name();

            let file_name = format!("{name}_{file_suffix}.mp4");

            let cmd = format!(
                "ffmpeg -y -f lavfi -i \"sine=frequency={freq}:sample_rate=48000:duration=20\" -af \"volume=3\" -c:a {encoder_name} -b:a 192k -ac 2 -ar 48000 -vn {file_name}"
            );

            Command::new("bash")
                .arg("-c")
                .arg(cmd)
                .current_dir(&snapshots_dir)
                .status()
                .unwrap();

            Command::new("bash")
                .arg("-c")
                .arg(format!(
                    "cargo run -p integration-tests --bin generate_rtp_from_file audio-{file_suffix} {file_name}"
                ))
                .current_dir(&snapshots_dir)
                .status()
                .unwrap();
        }
    }

    let wav_a = "ffmpeg -y -f lavfi -i \"sine=frequency=440.0:sample_rate=48000:duration=5\" -af \"volume=3\" -c:a pcm_s16le -ac 2 -ar 48000 -vn a.wav";
    let wav_c_sharp = "ffmpeg -y -f lavfi -i \"sine=frequency=554.37:sample_rate=48000:duration=15\" -af \"volume=3\" -c:a pcm_s16le -ac 2 -ar 48000 -vn c_sharp.wav";

    Command::new("bash")
        .arg("-c")
        .arg(wav_a)
        .current_dir(integration_tests_root())
        .status()
        .unwrap();
    Command::new("bash")
        .arg("-c")
        .arg(wav_c_sharp)
        .current_dir(integration_tests_root())
        .status()
        .unwrap();

    Command::new("bash")
        .arg("-c")
        .arg("sox a.wav c_sharp.wav variable_frequency.wav")
        .current_dir(integration_tests_root())
        .status()
        .unwrap();

    fs::remove_file(integration_tests_root().join("a.wav")).unwrap();
    fs::remove_file(integration_tests_root().join("c_sharp.wav")).unwrap();

    for encoder in encoders {
        let file_suffix = encoder.file_name();
        let encoder_name = encoder.name();

        let file_name = format!("variable_frequency_{file_suffix}.mp4");

        let cmd = format!(
            "ffmpeg -y -i variable_frequency.wav -c:a {encoder_name} -ac 2 -ar 48000 -vn {file_name}"
        );

        Command::new("bash")
            .arg("-c")
            .arg(&cmd)
            .current_dir(integration_tests_root())
            .status()
            .unwrap();

        Command::new("bash")
            .arg("-c")
            .arg(format!(
                "cargo run -p integration-tests --bin generate_rtp_from_file audio-{file_suffix} {file_name}"
            ))
            .current_dir(integration_tests_root())
            .status()
            .unwrap();

        fs::rename(
            integration_tests_root().join(&file_name),
            snapshots_dir.join(&file_name),
        )
        .unwrap();
    }
    fs::remove_file(integration_tests_root().join("variable_frequency.wav")).unwrap();
    fs::rename(
        integration_tests_root().join("variable_frequency_aac_audio_aac.rtp"),
        snapshots_dir.join("variable_frequency_aac_audio_aac.rtp"),
    )
    .unwrap();
    fs::rename(
        integration_tests_root().join("variable_frequency_opus_audio.rtp"),
        snapshots_dir.join("variable_frequency_opus_audio.rtp"),
    )
    .unwrap();
}
