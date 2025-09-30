use std::{
    fs::{self, create_dir_all},
    path::PathBuf,
    time::Duration,
};

use smelter_render::Resolution;

use crate::paths::failed_snapshots_dir_path;

use super::snapshot_save_path;

#[derive(Debug, Clone)]
pub(super) struct Snapshot {
    pub test_name: String,
    pub pts: Duration,
    pub resolution: Resolution,
    pub data: Vec<u8>,
}

impl Snapshot {
    pub(super) fn save_path(&self) -> PathBuf {
        snapshot_save_path(&self.test_name, &self.pts)
    }

    pub(super) fn diff_with_saved(&self) -> f32 {
        let save_path = self.save_path();
        if !save_path.exists() {
            return 1000.0;
        }
        let old_snapshot = image::open(save_path).unwrap().to_rgba8();
        snapshots_diff(&old_snapshot, &self.data)
    }

    pub(super) fn update_on_disk(&self) {
        let width = self.resolution.width - (self.resolution.width % 2);
        let height = self.resolution.height - (self.resolution.height % 2);
        let save_path = self.save_path();
        create_dir_all(save_path.parent().unwrap()).unwrap();
        image::save_buffer(
            save_path,
            &self.data,
            width as u32,
            height as u32,
            image::ColorType::Rgba8,
        )
        .unwrap();
    }

    pub(super) fn write_as_failed_snapshot(&self) {
        let failed_snapshot_path = failed_snapshots_dir_path();
        create_dir_all(&failed_snapshot_path).unwrap();

        let snapshot_name = self
            .save_path()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();

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

        fs::copy(
            self.save_path(),
            failed_snapshot_path.join(format!("expected_{snapshot_name}")),
        )
        .unwrap();
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
