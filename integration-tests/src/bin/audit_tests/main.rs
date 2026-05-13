use std::{fs, path::Path, process::Command};

use anyhow::{Context, Result};
use inquire::{Confirm, InquireError, Select};
use integration_tests::pipeline_tests::harness::SAVE_DUMPS_ENV;
use tracing::{error, info};

mod cleanup;
mod pipeline;
mod render;
mod restore_gh;
mod restore_submodule;

#[derive(Debug, Clone, Copy, strum::Display, strum::EnumIter)]
enum Action {
    #[strum(to_string = "Run all pipeline tests")]
    RunAll,
    #[strum(to_string = "Run specific pipeline test")]
    RunSpecific,
    #[strum(to_string = "Run all render tests")]
    RunAllRender,
    #[strum(to_string = "Run specific render test")]
    RunSpecificRender,
    #[strum(to_string = "Audit existing pipeline test results (no rerun)")]
    AuditExistingPipeline,
    #[strum(to_string = "Audit existing render test results (no rerun)")]
    AuditExistingRender,
    #[strum(to_string = "Restore test_workdir: from GitHub Actions (test_linux)")]
    DownloadCiArtifacts,
    #[strum(to_string = "Restore test_workdir: from snapshot submodule diff")]
    DiffSnapshotSubmodule,
    #[strum(to_string = "Cleanup orphan committed snapshots")]
    CleanupOrphanSnapshots,
}

fn main() -> Result<()> {
    use strum::IntoEnumIterator;

    tracing_subscriber::fmt().with_target(false).init();

    loop {
        let options: Vec<Action> = Action::iter().collect();
        println!();
        let choice = match Select::new("What would you like to do?", options).prompt() {
            Ok(c) => c,
            Err(InquireError::OperationCanceled | InquireError::OperationInterrupted) => {
                return Ok(());
            }
            Err(e) => return Err(e.into()),
        };

        let result = match choice {
            Action::RunAll => pipeline::run_all(),
            Action::RunSpecific => pipeline::run_specific(),
            Action::RunAllRender => render::run_all_render(),
            Action::RunSpecificRender => render::run_specific_render(),
            Action::AuditExistingPipeline => pipeline::audit_existing_pipeline(),
            Action::AuditExistingRender => render::audit_existing_render(),
            Action::DownloadCiArtifacts => restore_gh::download_ci_artifacts(),
            Action::DiffSnapshotSubmodule => restore_submodule::diff_snapshot_submodule(),
            Action::CleanupOrphanSnapshots => cleanup::cleanup_orphan_snapshots(),
        };
        if let Err(e) = result {
            error!("{e:#}");
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct RunOptions {
    /// Set [`SAVE_DUMPS_ENV`] in the child process so the harness
    /// always writes both expected/actual dumps to the workdir, even
    /// for tests that pass.
    pub(crate) save_dumps: bool,
}

pub(crate) fn run_test(filter: &str, options: RunOptions) -> Result<std::process::ExitStatus> {
    let mut cmd = Command::new("cargo");
    scrub_cargo_env(&mut cmd);
    cmd.args([
        "nextest",
        "run",
        "--profile",
        "audit",
        "-p",
        "integration-tests",
        "--lib",
        "--no-fail-fast",
        "-E",
        filter,
    ]);
    if options.save_dumps {
        cmd.env(SAVE_DUMPS_ENV, "1");
    }
    info!("> {cmd:?}");
    let status = cmd.status()?;
    if !status.success() {
        tracing::warn!("nextest exited with {status}");
    }
    Ok(status)
}

/// Removes CARGO_* vars that `cargo run` sets for our process, so a nested
/// cargo invocation doesn't misinterpret them and invalidate fingerprints.
/// Keeps user-configured vars like `CARGO_HOME` and `CARGO_TARGET_DIR`.
fn scrub_cargo_env(cmd: &mut Command) {
    for (key, _) in std::env::vars() {
        let strip = key == "CARGO"
            || key == "OUT_DIR"
            || key.starts_with("CARGO_PKG_")
            || key.starts_with("CARGO_CRATE_")
            || key.starts_with("CARGO_BIN_")
            || key.starts_with("CARGO_MANIFEST_")
            || key == "CARGO_PRIMARY_PACKAGE"
            || key == "CARGO_TARGET_TMPDIR";
        if strip {
            cmd.env_remove(&key);
        }
    }
}

/// Confirm overwriting `dir`. Skips the prompt entirely when there's
/// nothing to wipe (dir doesn't exist or is empty). Otherwise asks
/// `message` with a "yes" default — these wipes are launched
/// intentionally, so a single Enter is enough. Cancelling (Esc /
/// Ctrl-C) is treated as "no".
pub(crate) fn confirm_wipe(dir: &Path, message: &str) -> Result<bool> {
    let has_content = fs::read_dir(dir)
        .map(|mut entries| entries.next().is_some())
        .unwrap_or(false);
    if !has_content {
        return Ok(true);
    }
    println!();
    match Confirm::new(message).with_default(true).prompt() {
        Ok(b) => Ok(b),
        Err(InquireError::OperationCanceled | InquireError::OperationInterrupted) => Ok(false),
        Err(e) => Err(e.into()),
    }
}

pub(crate) fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

pub(crate) fn walk_dir(dir: &Path, visit: &mut dyn FnMut(&Path)) -> Result<()> {
    for entry in fs::read_dir(dir).with_context(|| format!("Failed to read {}", dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            walk_dir(&path, visit)?;
        } else {
            visit(&path);
        }
    }
    Ok(())
}
