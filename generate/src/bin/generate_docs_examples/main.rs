use std::{env, path::PathBuf};

use basic_layouts::generate_basic_layouts_guide;
use quick_start::generate_quick_start_guide;
use transition::generate_tile_transition_video;
use view_transitions::generate_view_transition_guide;

mod basic_layouts;
mod quick_start;
mod transition;
mod view_transitions;

fn main() {
    let root_path = PathBuf::from(env::var("DOCS_GENERATED_DIR").unwrap());
    generate_quick_start_guide(&root_path).unwrap();
    generate_basic_layouts_guide(&root_path).unwrap();
    generate_tile_transition_video(&root_path).unwrap();
    generate_view_transition_guide(&root_path).unwrap();
}

fn workingdir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("workingdir")
        .join("inputs")
}
