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

/// Root directory for all transient artifacts produced by the test
/// suite — failed snapshot dumps, decoded debug bundles, etc. Cleaned
/// up out-of-band; nothing here is committed.
pub fn test_workdir() -> PathBuf {
    integration_tests_root().join("test_workdir")
}

/// Per-suite subdirectory under [`test_workdir`] for pipeline tests
/// (RTP dump comparisons driven by `audit_tests`).
pub fn pipeline_tests_workdir() -> PathBuf {
    test_workdir().join("pipeline_tests")
}

/// Per-suite subdirectory under [`test_workdir`] for render tests
/// (image snapshot comparisons).
pub fn render_tests_workdir() -> PathBuf {
    test_workdir().join("render_tests")
}
