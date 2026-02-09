use std::{fs, io, path::PathBuf};

use schemars::{
    JsonSchema,
    schema::{RootSchema, Schema, SchemaObject},
    schema_for,
};
use serde::{Deserialize, Serialize};
use smelter::routes;
use utoipa::OpenApi;

const ROOT_DIR: &str = env!("CARGO_MANIFEST_DIR");

fn main() {
    tracing_subscriber::fmt().init();
    let check_flag = std::env::args().any(|arg| &arg == "--check");
    generate_json_schema(check_flag);
    generate_openapi();
}

/// This enum is used to generate JSON schema for all API types.
/// This prevents repeating types in generated schema.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
#[allow(dead_code)]
enum ApiTypes {
    RegisterInput(routes::register_request::RegisterInput),
    RegisterOutput(Box<routes::register_request::RegisterOutput>),
    RegisterImage(smelter_api::ImageSpec),
    RegisterWebRenderer(smelter_api::WebRendererSpec),
    RegisterShader(smelter_api::ShaderSpec),
    UpdateOutput(Box<routes::update_output::UpdateOutputRequest>),
}

pub fn generate_json_schema(check_flag: bool) {
    let (scene_schema_action, api_schema_action) = match check_flag {
        true => (SchemaAction::CheckIfChanged, SchemaAction::Nothing),
        false => (SchemaAction::Update, SchemaAction::Update),
    };
    generate_schema(
        schema_for!(routes::update_output::UpdateOutputRequest),
        "./schemas/scene.schema.json",
        scene_schema_action,
    );
    generate_schema(
        schema_for!(ApiTypes),
        "./schemas/api_types.schema.json",
        api_schema_action,
    );
}

/// When variant inside oneOf has a schema additionalProperties set to false then
/// all the values outside of the variant are not allowed.
///
/// This function copies all the entries from `properties` to `oneOf[variant].properties`.
fn flatten_definitions_with_one_of(schema: &mut RootSchema) {
    for (_, schema) in schema.definitions.iter_mut() {
        match schema {
            Schema::Bool(_) => (),
            Schema::Object(definition) => flatten_definition_with_one_of(definition),
        }
    }
}

fn flatten_definition_with_one_of(definition: &mut SchemaObject) {
    let Some(ref properties) = definition.object.clone() else {
        return;
    };

    let Some(ref mut one_of) = definition.subschemas().one_of else {
        return;
    };

    for variant in one_of.iter_mut() {
        match variant {
            Schema::Bool(_) => (),
            Schema::Object(variant) => {
                for (prop_name, prop) in properties.properties.iter() {
                    variant
                        .object()
                        .properties
                        .insert(prop_name.clone(), prop.clone());
                }
            }
        }
    }
}

fn generate_schema(mut current_schema: RootSchema, path: &'static str, action: SchemaAction) {
    flatten_definitions_with_one_of(&mut current_schema);

    let root_dir: PathBuf = ROOT_DIR.into();
    let schema_path = root_dir.join(path);
    fs::create_dir_all(schema_path.parent().unwrap()).unwrap();

    let json_from_disk = match fs::read_to_string(&schema_path) {
        Ok(json) => json,
        Err(err) if err.kind() == io::ErrorKind::NotFound => String::new(),
        Err(err) => panic!("{}", err),
    };
    let json_current = serde_json::to_string_pretty(&current_schema).unwrap() + "\n";

    if json_current != json_from_disk {
        match action {
            SchemaAction::Update => fs::write(schema_path, &json_current).unwrap(),
            SchemaAction::CheckIfChanged => {
                panic!("Schema changed. Rerun without --check arg to regenerate it.")
            }
            SchemaAction::Nothing => (),
        };
    }
}

enum SchemaAction {
    Update,
    CheckIfChanged,
    Nothing,
}

// OpenAPI specification generation

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

fn generate_openapi() {
    let openapi_json = ApiDoc::openapi().to_pretty_json().unwrap();
    let gen_path = PathBuf::from(ROOT_DIR).join("schemas/openapi_specification.json");
    fs::write(&gen_path, openapi_json).unwrap();
}
