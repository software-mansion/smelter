use std::{
    fs::{self, create_dir_all},
    path::PathBuf,
    time::Duration,
};

use smelter_render::Resolution;

use crate::paths::render_tests_workdir;

use super::snapshot_save_path;

#[derive(Debug, Clone)]
pub(crate) struct Snapshot {
    /// Module the test lives in — also the subdirectory under
    /// `render_snapshots/` where the committed snapshot lives.
    pub module: String,
    /// Test name — also the prefix of the snapshot file name.
    pub test_name: String,
    pub pts: Duration,
    pub resolution: Resolution,
    pub data: Vec<u8>,
}

impl Snapshot {
    pub(super) fn save_path(&self) -> PathBuf {
        snapshot_save_path(&self.module, &self.test_name, &self.pts)
    }

    pub(super) fn diff_with_saved(&self) -> f32 {
        let save_path = self.save_path();
        if !save_path.exists() {
            return 1000.0;
        }
        let old_snapshot = image::open(save_path).unwrap().to_rgba8();
        snapshots_diff(&old_snapshot, &self.data)
    }

    /// Workdir is flat, so the module name is encoded into the
    /// file name (separated by `__`) — otherwise the audit tool
    /// can't tell which `<test.module>` a workdir file belongs
    /// to, and same-named tests in different modules would alias.
    fn workdir_snapshot_name(&self) -> String {
        format!(
            "{}__{}_{:05}_{}.png",
            self.module,
            self.test_name,
            self.pts.as_millis(),
            super::OUTPUT_ID,
        )
    }

    pub(super) fn write_as_failed_snapshot(&self) {
        let failed_snapshot_path = render_tests_workdir();
        create_dir_all(&failed_snapshot_path).unwrap();

        let snapshot_name = self.workdir_snapshot_name();

        let width = self.resolution.width - (self.resolution.width % 2);
        let height = self.resolution.height - (self.resolution.height % 2);
        image::save_buffer(
            failed_snapshot_path.join(format!("actual_{snapshot_name}")),
            &self.data,
            width as u32,
            height as u32,
            image::ColorType::Rgba8,
        )
        .unwrap();

        let _ = fs::copy(
            self.save_path(),
            failed_snapshot_path.join(format!("expected_{snapshot_name}")),
        );
    }

    /// Remove workdir files for this snapshot left by a previous
    /// failed run. Called on a passing run so stale failure artifacts
    /// don't linger for the audit tooling to pick up.
    pub(super) fn clear_failed_snapshot(&self) {
        let failed_snapshot_path = render_tests_workdir();
        let snapshot_name = self.workdir_snapshot_name();
        for prefix in ["actual_", "expected_"] {
            let file = failed_snapshot_path.join(format!("{prefix}{snapshot_name}"));
            if file.exists()
                && let Err(e) = fs::remove_file(&file)
            {
                println!("Failed to remove stale snapshot {}: {e}", file.display());
            }
        }
    }
}

fn snapshots_diff(old_snapshot: &[u8], new_snapshot: &[u8]) -> f32 {
    if old_snapshot.len() != new_snapshot.len() {
        return 10000.0;
    }
    let square_error: f32 = old_snapshot
        .iter()
        .zip(new_snapshot)
        .map(|(a, b)| (*a as i32 - *b as i32).pow(2) as f32)
        .sum();

    square_error / old_snapshot.len() as f32
}
