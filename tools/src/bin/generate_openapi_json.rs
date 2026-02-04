use std::{fs, path::PathBuf};

use smelter_api::RtpInput;
use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(components(schemas(RtpInput)))]
struct ApiDoc;

const ROOT_DIR: &str = env!("CARGO_MANIFEST_DIR");

fn main() {
    tracing_subscriber::fmt().init();

    let schema_json = ApiDoc::openapi().to_pretty_json().unwrap();
    let gen_path = PathBuf::from(ROOT_DIR).join("schemas/openapi.json");
    fs::write(&gen_path, schema_json).unwrap();
}
