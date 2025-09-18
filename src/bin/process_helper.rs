use std::error::Error;

// Subprocess used by chromium
fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt().json().init();

    let exit_code = smelter_render::web_renderer::process_helper::run_process_helper()?;
    std::process::exit(exit_code);
}
