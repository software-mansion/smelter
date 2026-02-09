use std::{fs, path::PathBuf};

use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Smelter",
        description = "Real-time video compositing software",
        version = "0.5.0",
        license(
            name = "",
            url = "https://github.com/software-mansion/smelter/blob/master/LICENSE",
        ),
    ),
    paths(
        smelter::routes::control_request::handle_start,
        smelter::routes::control_request::handle_reset,
        smelter::routes::register_request::handle_input,
        smelter::routes::register_request::handle_output,
        smelter::routes::register_request::handle_shader,
        smelter::routes::register_request::handle_web_renderer,
        smelter::routes::register_request::handle_image,
        smelter::routes::register_request::handle_font,
        smelter::routes::unregister_request::handle_input,
        smelter::routes::unregister_request::handle_output,
        smelter::routes::unregister_request::handle_shader,
        smelter::routes::unregister_request::handle_web_renderer,
        smelter::routes::unregister_request::handle_image,
        smelter::routes::update_output::handle_output_update,
        smelter::routes::update_output::handle_keyframe_request,
        smelter::routes::status::status_handler,
        smelter::routes::ws::ws_handler,
    )
)]
struct ApiDoc;

const ROOT_DIR: &str = env!("CARGO_MANIFEST_DIR");

fn main() {
    tracing_subscriber::fmt().init();

    let schema_json = ApiDoc::openapi().to_pretty_json().unwrap();

    let gen_path = PathBuf::from(ROOT_DIR).join("openapi.json");
    fs::write(&gen_path, schema_json).unwrap();
}
