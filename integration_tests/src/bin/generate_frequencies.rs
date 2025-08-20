use std::process::Command;

fn main() {
    let encoders = vec!["aac", "libopus"];
    let notes = vec![("a", 440.0), ("c_sharp", 554.37), ("e", 659.26)];

    for encoder in &encoders {
        for (name, freq) in &notes {
            let cmd = format!("ffmpeg -f lavfi -i \"sine=frequency={freq}:sample_rate=48000:duration=20\" -af \"volume=0.4\" -c:a {encoder} -b:a 192k -ac 2 -ar 48000 -vn {name}_{encoder}.mp4");

            Command::new("bash").arg("-c").arg(cmd).status().unwrap();
        }
    }
}
