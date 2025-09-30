use std::path::PathBuf;

pub fn integration_tests_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

pub fn submodule_root_path() -> PathBuf {
    integration_tests_root().join("snapshots")
}

pub fn render_snapshots_dir_path() -> PathBuf {
    submodule_root_path().join("render_snapshots")
}

pub fn failed_snapshots_dir_path() -> PathBuf {
    integration_tests_root().join("failed_snapshot_tests")
}
