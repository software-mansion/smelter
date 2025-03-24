use std::error::Error;

// Subprocess used by chromium
fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt().json().init();

    let exit_code = compositor_render::web_renderer::process_helper::run_process_helper()?;
    std::process::exit(exit_code);
}
