use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result};
use inquire::{InquireError, Select};
use integration_tests::{
    paths::{pipeline_tests_workdir, render_tests_workdir, submodule_root_path},
    pipeline_tests::pipeline_tests,
};
use tracing::{info, warn};

use crate::{confirm_wipe, truncate, walk_dir};

/// Populate the pipeline and render workdirs from a `git diff`
/// between the snapshot submodule's current working tree (treated as
/// `actual`) and a chosen past commit (treated as `expected`). Only
/// files that changed bit-for-bit are written, so the audit UI lists
/// exactly the snapshots that need a re-look after a snapshot update.
pub(crate) fn diff_snapshot_submodule() -> Result<()> {
    let submodule = submodule_root_path();
    if !submodule.exists() {
        anyhow::bail!(
            "Snapshot submodule not initialized at {}. Run `git submodule update --init --checkout integration-tests/snapshots`.",
            submodule.display()
        );
    }

    // Refresh remote refs first so `origin/main` (and the rest of the
    // picker) reflects the latest upstream state. `--quiet` keeps the
    // output clean; `--tags --prune` makes sure deleted refs disappear.
    info!("Fetching {} ...", submodule.display());
    fetch_origin(&submodule)?;

    let commits = list_snapshot_commits(&submodule)?;
    if commits.is_empty() {
        warn!("No commits found in {}", submodule.display());
        return Ok(());
    }

    let labels: Vec<String> = commits.iter().map(SnapshotCommit::label).collect();
    println!();
    let selected_idx = match Select::new(
        "Select a past commit to diff current snapshots against:",
        labels,
    )
    .with_page_size(15)
    .raw_prompt()
    {
        Ok(s) => s.index,
        Err(InquireError::OperationCanceled | InquireError::OperationInterrupted) => {
            return Ok(());
        }
        Err(e) => return Err(e.into()),
    };
    let commit = &commits[selected_idx];

    let pipeline_dest = pipeline_tests_workdir();
    let render_dest = render_tests_workdir();
    let short_sha = commit.short_sha();
    if !confirm_wipe(
        &pipeline_dest,
        &format!(
            "Wipe {} and populate from pipeline dumps that differ from {}?",
            pipeline_dest.display(),
            short_sha
        ),
    )? {
        return Ok(());
    }
    if !confirm_wipe(
        &render_dest,
        &format!(
            "Wipe {} and populate from render snapshots that differ from {}?",
            render_dest.display(),
            short_sha
        ),
    )? {
        return Ok(());
    }

    reset_workdir(&pipeline_dest)?;
    reset_workdir(&render_dest)?;

    let pipeline_summary =
        restore_pipeline_snapshots(&submodule, &commit.sha, &pipeline_dest)?;
    let render_summary = restore_render_snapshots(&submodule, &commit.sha, &render_dest)?;

    info!(
        "Pipeline: wrote {} differing dump(s) into {} \
         (identical: {}, missing current: {}, missing at {}: {})",
        pipeline_summary.written,
        pipeline_dest.display(),
        pipeline_summary.skipped_identical,
        pipeline_summary.skipped_missing_current,
        commit.short_sha(),
        pipeline_summary.skipped_missing_old,
    );
    info!(
        "Render: wrote {} differing snapshot(s) into {} \
         (identical: {}, missing at {}: {})",
        render_summary.written,
        render_dest.display(),
        render_summary.skipped_identical,
        commit.short_sha(),
        render_summary.skipped_missing_old,
    );
    Ok(())
}

#[derive(Default)]
struct DiffSummary {
    written: usize,
    skipped_identical: usize,
    skipped_missing_current: usize,
    skipped_missing_old: usize,
}

fn reset_workdir(dest: &Path) -> Result<()> {
    if dest.exists() {
        fs::remove_dir_all(dest)
            .with_context(|| format!("Failed to clear {}", dest.display()))?;
    }
    fs::create_dir_all(dest)
        .with_context(|| format!("Failed to create {}", dest.display()))?;
    Ok(())
}

/// Walk every registered `PipelineTest`, compare its committed dump
/// against the chosen commit, and write the diffs to the pipeline
/// workdir using the harness's `actual_dump_<name>` / `expected_dump_<name>`
/// naming.
fn restore_pipeline_snapshots(
    submodule: &Path,
    commit_sha: &str,
    dest: &Path,
) -> Result<DiffSummary> {
    let mut summary = DiffSummary::default();
    for test in pipeline_tests() {
        let rel = format!("rtp_packet_dumps/outputs/{}", test.snapshot_name);
        let current_path = submodule.join(&rel);
        let current = match fs::read(&current_path) {
            Ok(b) => b,
            Err(_) => {
                summary.skipped_missing_current += 1;
                continue;
            }
        };
        let old = match read_blob_at(submodule, commit_sha, &rel) {
            Some(b) => b,
            None => {
                summary.skipped_missing_old += 1;
                continue;
            }
        };
        if current == old {
            summary.skipped_identical += 1;
            continue;
        }
        fs::write(dest.join(format!("actual_dump_{}", test.snapshot_name)), &current)?;
        fs::write(dest.join(format!("expected_dump_{}", test.snapshot_name)), &old)?;
        summary.written += 1;
    }
    Ok(summary)
}

/// Walk every committed render snapshot, compare against the chosen
/// commit, and write differing ones to the render workdir using the
/// harness's `actual_<module>__<name>` / `expected_<module>__<name>`
/// naming. Deletions (files that existed at the old commit but not
/// now) are not surfaced — same as for pipeline dumps, since the
/// audit UI keys off the current set of files.
fn restore_render_snapshots(
    submodule: &Path,
    commit_sha: &str,
    dest: &Path,
) -> Result<DiffSummary> {
    let render_root = submodule.join("render_snapshots");
    let mut summary = DiffSummary::default();
    if !render_root.exists() {
        return Ok(summary);
    }
    let mut files: Vec<PathBuf> = Vec::new();
    walk_dir(&render_root, &mut |path| {
        if path.extension().is_some_and(|e| e == "png") {
            files.push(path.to_path_buf());
        }
    })?;
    for path in files {
        // `render_snapshots/<module>/<file_name>` — split for the
        // workdir filename + the `git show` rel-path.
        let Ok(rel_under_render) = path.strip_prefix(&render_root) else {
            continue;
        };
        let mut components = rel_under_render.components();
        let Some(module) = components.next().and_then(|c| c.as_os_str().to_str()) else {
            // Files sitting directly in `render_snapshots/` aren't
            // owned by any test — ignore.
            continue;
        };
        let Some(file_name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let rel = format!(
            "render_snapshots/{}",
            rel_under_render.to_string_lossy().replace('\\', "/")
        );
        let current = match fs::read(&path) {
            Ok(b) => b,
            Err(_) => {
                summary.skipped_missing_current += 1;
                continue;
            }
        };
        let old = match read_blob_at(submodule, commit_sha, &rel) {
            Some(b) => b,
            None => {
                summary.skipped_missing_old += 1;
                continue;
            }
        };
        if current == old {
            summary.skipped_identical += 1;
            continue;
        }
        let workdir_name = format!("{module}__{file_name}");
        fs::write(dest.join(format!("actual_{workdir_name}")), &current)?;
        fs::write(dest.join(format!("expected_{workdir_name}")), &old)?;
        summary.written += 1;
    }
    Ok(summary)
}

#[derive(Debug)]
struct SnapshotCommit {
    sha: String,
    date: String,
    subject: String,
    /// When `Some`, the picker shows this in place of the short sha
    /// — used to surface the synthetic `origin/main` entry that
    /// resolves to whatever the remote points at.
    ref_name: Option<String>,
}

impl SnapshotCommit {
    fn short_sha(&self) -> &str {
        self.sha.get(..7).unwrap_or(&self.sha)
    }

    fn label(&self) -> String {
        let date = self.date.get(..16).unwrap_or(&self.date);
        let head = match &self.ref_name {
            Some(name) => format!("{name} ({})", self.short_sha()),
            None => self.short_sha().to_string(),
        };
        format!("{head} | {date} | {}", truncate(&self.subject, 70))
    }
}

fn list_snapshot_commits(submodule: &std::path::Path) -> Result<Vec<SnapshotCommit>> {
    let output = Command::new("git")
        .args([
            "-C",
            &submodule.display().to_string(),
            "log",
            "-30",
            "--pretty=format:%H%x09%cI%x09%s",
        ])
        .output()
        .context("Failed to spawn `git log`")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("`git log` exited with {}: {stderr}", output.status);
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let log_commits: Vec<SnapshotCommit> = stdout
        .lines()
        .filter_map(|line| {
            let mut parts = line.splitn(3, '\t');
            let sha = parts.next()?.to_string();
            let date = parts.next()?.to_string();
            let subject = parts.next().unwrap_or("").to_string();
            Some(SnapshotCommit { sha, date, subject, ref_name: None })
        })
        .collect();

    // Build the picker top: the most-frequently-used revisions, in
    // priority order. `git log -30` provides the rest. Pinned entries
    // shadow themselves out of the log to avoid duplicates.
    let mut pinned: Vec<SnapshotCommit> = Vec::new();
    let mut push_pinned = |commit: Option<SnapshotCommit>| {
        if let Some(c) = commit
            && !pinned.iter().any(|p| p.sha == c.sha)
        {
            pinned.push(c);
        }
    };
    push_pinned(resolve_remote_default(submodule));
    push_pinned(resolve_named(submodule, "HEAD"));
    push_pinned(resolve_named(submodule, "HEAD~1"));

    let pinned_shas: std::collections::HashSet<String> =
        pinned.iter().map(|c| c.sha.clone()).collect();
    let mut result = pinned;
    result.extend(log_commits.into_iter().filter(|c| !pinned_shas.contains(&c.sha)));
    Ok(result)
}

/// Try `origin/main` then `origin/master`; return the first that
/// resolves to a commit, with metadata fetched via `git log -1`.
fn resolve_remote_default(submodule: &std::path::Path) -> Option<SnapshotCommit> {
    for name in ["origin/main", "origin/master"] {
        if let Some(commit) = resolve_named(submodule, name) {
            return Some(commit);
        }
    }
    None
}

fn fetch_origin(submodule: &std::path::Path) -> Result<()> {
    let status = Command::new("git")
        .args([
            "-C",
            &submodule.display().to_string(),
            "fetch",
            "--quiet",
            "--tags",
            "--prune",
            "origin",
        ])
        .status()
        .context("Failed to spawn `git fetch`")?;
    if !status.success() {
        // A failed fetch is usually transient (offline, auth);
        // surface it as a warning and let the user pick from
        // whatever's already on disk rather than abort the flow.
        warn!(
            "`git fetch origin` exited with {status} — picker will show pre-fetch state of {}",
            submodule.display()
        );
    }
    Ok(())
}

/// Resolve a single named revision (e.g. `HEAD`, `HEAD~1`,
/// `origin/main`) and tag the resulting commit with `name` so the
/// picker shows `name (sha7)` instead of just the short sha.
fn resolve_named(submodule: &std::path::Path, name: &str) -> Option<SnapshotCommit> {
    let commit = log_one(submodule, name)?;
    Some(SnapshotCommit { ref_name: Some(name.to_string()), ..commit })
}

fn log_one(submodule: &std::path::Path, revision: &str) -> Option<SnapshotCommit> {
    let output = Command::new("git")
        .args([
            "-C",
            &submodule.display().to_string(),
            "log",
            "-1",
            "--pretty=format:%H%x09%cI%x09%s",
            revision,
            "--",
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let line = String::from_utf8_lossy(&output.stdout);
    let line = line.trim_end_matches('\n');
    let mut parts = line.splitn(3, '\t');
    Some(SnapshotCommit {
        sha: parts.next()?.to_string(),
        date: parts.next()?.to_string(),
        subject: parts.next().unwrap_or("").to_string(),
        ref_name: None,
    })
}

/// Read a single file at `relative_path` as it existed at `sha` in
/// the submodule's history. Returns `None` if the file didn't exist
/// at that revision (or `git show` failed for any other reason).
fn read_blob_at(
    submodule: &std::path::Path,
    sha: &str,
    relative_path: &str,
) -> Option<Vec<u8>> {
    let output = Command::new("git")
        .args([
            "-C",
            &submodule.display().to_string(),
            "show",
            &format!("{sha}:{relative_path}"),
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(output.stdout)
}
