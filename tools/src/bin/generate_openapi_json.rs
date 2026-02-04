use std::{fs, path::PathBuf};

use smelter_api::{HlsInput, Mp4Input, RtmpInput, RtpInput, V4l2Input, WhepInput, WhipInput};
use smelter_api::{HlsOutput, Mp4Output, RtmpOutput, RtpOutput, WhepOutput, WhipOutput};
use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(components(schemas(
    HlsInput, Mp4Input, RtmpInput, RtpInput, V4l2Input, WhepInput, WhipInput, HlsOutput, Mp4Output,
    RtmpOutput, RtpOutput, WhepOutput, WhipOutput
)))]
struct ApiDoc;

const ROOT_DIR: &str = env!("CARGO_MANIFEST_DIR");

fn main() {
    tracing_subscriber::fmt().init();

    let schema_json = ApiDoc::openapi().to_pretty_json().unwrap();

    let gen_path = PathBuf::from(ROOT_DIR).join("openapi.json");
    fs::write(&gen_path, schema_json).unwrap();
}
