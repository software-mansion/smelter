use std::error::Error;

use app::App;

mod app;
mod handler;
mod state;

pub fn run_process_helper() -> Result<i32, Box<dyn Error>> {
    let app = App::new();
    let context = libcef::Context::new_helper()?;
    let exit_code = context.execute_process(app);
    Ok(exit_code)
}
