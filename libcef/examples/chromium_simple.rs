use std::{fs, path::Path, process::Stdio};

use libcef::Resolution;

fn bgra_to_png(
    input_file: impl AsRef<Path>,
    output_file: impl AsRef<Path>,
    resolution: libcef::Resolution,
) {
    std::process::Command::new("ffmpeg")
        .arg("-f")
        .arg("rawvideo")
        .arg("-pix_fmt")
        .arg("bgra")
        .arg("-video_size")
        .arg(format!("{}x{}", resolution.width, resolution.height))
        .arg("-i")
        .arg(input_file.as_ref().as_os_str())
        .arg(output_file.as_ref().as_os_str())
        .arg("-y")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn ffmpeg")
        .wait()
        .expect("wait");
}

struct App;

impl libcef::App for App {
    type RenderProcessHandlerType = ();

    fn on_before_command_line_processing(
        &mut self,
        process_type: String,
        command_line: &mut libcef::CommandLine,
    ) {
        // Check if main process
        if !process_type.is_empty() {
            return;
        }

        #[cfg(target_os = "macos")]
        command_line.append_switch("use-mock-keychain");
        command_line.append_switch("disable-gpu");
        command_line.append_switch("disable-gpu-shader-disk-cache");
        command_line.append_switch("show-fps-counter");
    }
}

struct Client;

impl libcef::Client for Client {
    type RenderHandlerType = RenderHandler;

    fn render_handler(&self) -> Option<Self::RenderHandlerType> {
        Some(RenderHandler)
    }
}

struct RenderHandler;

impl libcef::RenderHandler for RenderHandler {
    fn resolution(&self, _browser: &libcef::Browser) -> Resolution {
        Resolution {
            width: 1920,
            height: 1080,
        }
    }

    fn on_paint(&self, browser: &libcef::Browser, buffer: &[u8], resolution: Resolution) {
        if !browser.is_loading().expect("valid browser") {
            fs::write("out.raw", buffer).expect("save image buffer");
            bgra_to_png("out.raw", "out.png", resolution);
            fs::remove_file("./out.raw").expect("remove raw image file");
        }
    }
}

fn main() {
    let target_path = &std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .join("..");

    if libcef::bundle_for_development(target_path).is_err() {
        panic!(
            "Build process helper first. For release profile use: cargo build -r --bin process_helper"
        );
    }

    let app = App;
    let settings = libcef::Settings {
        windowless_rendering_enabled: true,
        log_severity: libcef::LogSeverity::Info,
        ..Default::default()
    };

    let ctx = libcef::Context::new(app, settings).expect("create browser");

    let client = Client;
    let window_info = libcef::WindowInfo {
        windowless_rendering_enabled: true,
    };
    let browser_settings = libcef::BrowserSettings {
        windowless_frame_rate: 60,
        background_color: 0xfff,
    };
    let _ = ctx.start_browser(
        client,
        window_info,
        browser_settings,
        "https://membrane.stream",
    );

    println!("Starting image generation");
    ctx.run_message_loop();
}
