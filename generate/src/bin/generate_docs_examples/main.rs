use std::{env, path::PathBuf};

use basic_layouts::generate_basic_layouts_guide;
use component_image::generate_image_component_example;
use component_input_stream::generate_input_stream_component_example;
use component_mp4::generate_mp4_component_example;
use component_rescaler::generate_rescaler_component_example;
use component_shader::generate_shader_component_example;
use component_show::generate_show_component_example;
use component_slide_show::generate_slide_show_component_example;
use component_text::generate_text_component_example;
use component_tiles::generate_tile_component_example;
use component_view::generate_view_component_example;
use quick_start::generate_quick_start_guide;
use view_transitions::generate_view_transition_guide;

mod basic_layouts;
mod quick_start;
mod view_transitions;

mod component_image;
mod component_input_stream;
mod component_mp4;
mod component_rescaler;
mod component_shader;
mod component_show;
mod component_slide_show;
mod component_text;
mod component_tiles;
mod component_view;

fn main() {
    let root_path = PathBuf::from(env::var("DOCS_GENERATED_DIR").unwrap());

    //// guides
    generate_quick_start_guide(&root_path).unwrap();
    generate_basic_layouts_guide(&root_path).unwrap();
    generate_view_transition_guide(&root_path).unwrap();

    // components
    generate_view_component_example(&root_path).unwrap();
    generate_image_component_example(&root_path).unwrap();
    generate_input_stream_component_example(&root_path).unwrap();
    generate_rescaler_component_example(&root_path).unwrap();
    generate_shader_component_example(&root_path).unwrap();
    generate_mp4_component_example(&root_path).unwrap();
    generate_show_component_example(&root_path).unwrap();
    generate_slide_show_component_example(&root_path).unwrap();
    generate_text_component_example(&root_path).unwrap();
    generate_tile_component_example(&root_path).unwrap();
}

fn workingdir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("workingdir")
        .join("inputs")
}

fn examples_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}
