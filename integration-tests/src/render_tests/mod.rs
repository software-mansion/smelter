mod harness;

mod image;
mod rescaler;
mod shader;
mod simple;
mod text;
mod tiles;
mod tiles_transitions;
mod transition;
mod view;

#[cfg(test)]
mod pixel_input_format_tests;
#[cfg(test)]
mod yuv_tests;

#[derive(Debug)]
pub struct RenderTest {
    /// Name of the test — the test function's identifier.
    pub test_name: &'static str,
    pub full_test_name: &'static str,
    pub description: &'static str,
    pub test_fn: fn() -> anyhow::Result<()>,
    /// Module the test is defined in. Auto-derived from
    /// `module_path!()` (last `::`-separated segment).
    pub module: &'static str,
}

/// Extracts the last `::`-separated segment of a `module_path!()` value.
/// E.g. `"integration_tests::render_tests::simple"` → `"simple"`.
pub const fn module_from_module_path(module_path: &str) -> &str {
    let bytes = module_path.as_bytes();
    let len = bytes.len();

    let mut last_sep = 0;
    let mut i = 0;
    while i + 1 < len {
        if bytes[i] == b':' && bytes[i + 1] == b':' {
            last_sep = i + 2;
        }
        i += 1;
    }

    let (_, module_name) = module_path.split_at(last_sep);
    module_name
}

pub fn render_tests() -> Vec<&'static RenderTest> {
    [
        simple::TESTS,
        image::TESTS,
        rescaler::TESTS,
        shader::TESTS,
        text::TESTS,
        tiles::TESTS,
        tiles_transitions::TESTS,
        transition::TESTS,
        view::TESTS,
    ]
    .iter()
    .flat_map(|tests| tests.iter())
    .collect()
}
