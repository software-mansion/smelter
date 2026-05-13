use std::path::PathBuf;

use crate::paths::render_snapshots_dir_path;

pub(crate) mod input;
pub(crate) mod snapshot;
pub(crate) mod test_case;
mod utils;

pub(crate) const OUTPUT_ID: &str = "output_1";

#[allow(dead_code)]
pub(crate) const DEFAULT_RESOLUTION: smelter_render::Resolution = smelter_render::Resolution {
    width: 640,
    height: 360,
};

pub(super) fn save_dumps_env_set() -> bool {
    std::env::var_os("SMELTER_SAVE_DUMPS").is_some_and(|v| !v.is_empty())
}

fn snapshot_save_path(module: &str, test_name: &str, pts: &std::time::Duration) -> PathBuf {
    let pts = pts.as_millis();
    render_snapshots_dir_path()
        .join(module)
        .join(format!("{test_name}_{pts:05}_{OUTPUT_ID}.png"))
}
