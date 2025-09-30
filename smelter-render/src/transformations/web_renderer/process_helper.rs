use std::error::Error;

use app::App;
use libcef::cef;

mod app;
mod handler;
mod state;

pub fn run_process_helper() -> Result<i32, Box<dyn Error>> {
    let app = App::new();
    let context = cef::Context::new_helper()?;
    let exit_code = context.execute_process(app);
    Ok(exit_code)
}
