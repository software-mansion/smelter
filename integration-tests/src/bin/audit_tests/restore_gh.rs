use std::{
    fs,
    process::{Command, Stdio},
};

use anyhow::{Context, Result};
use inquire::{InquireError, Select};
use integration_tests::paths::test_workdir;
use tracing::{info, warn};

use crate::{confirm_wipe, truncate};

/// Replace `test_workdir/` with the `test_workdir` artifact attached
/// to a chosen CI run. Useful for triaging CI failures locally
/// without having to wait for the test to fail again on this machine.
///
/// Shells out to `gh` (must be installed and authenticated). Listing
/// uses the artifacts API filtered by name (so only runs that
/// actually have a downloadable `test_workdir` artifact appear);
/// downloading uses `gh run download`.
pub(crate) fn download_ci_artifacts() -> Result<()> {
    if !gh_available() {
        anyhow::bail!(
            "`gh` not found in PATH. Install GitHub CLI and `gh auth login` to download CI artifacts."
        );
    }

    let runs = list_recent_runs()?;
    if runs.is_empty() {
        warn!("No CI runs with a `test_workdir` artifact were found");
        return Ok(());
    }

    let labels: Vec<String> = runs.iter().map(ArtifactRun::label).collect();
    println!();
    let selected_idx = match Select::new("Select a CI run to pull dumps from:", labels)
        .with_page_size(15)
        .raw_prompt()
    {
        Ok(s) => s.index,
        Err(InquireError::OperationCanceled | InquireError::OperationInterrupted) => {
            return Ok(());
        }
        Err(e) => return Err(e.into()),
    };
    let run = &runs[selected_idx];

    let dest = test_workdir();
    info!(
        "Replacing {} with artifact `test_workdir` from run #{}",
        dest.display(),
        run.run_id
    );

    if !confirm_wipe(
        &dest,
        &format!("Wipe {} and overwrite with CI artifact?", dest.display()),
    )? {
        return Ok(());
    }

    if dest.exists() {
        fs::remove_dir_all(&dest)
            .with_context(|| format!("Failed to clear {}", dest.display()))?;
    }
    fs::create_dir_all(&dest)
        .with_context(|| format!("Failed to create {}", dest.display()))?;

    let mut cmd = Command::new("gh");
    cmd.args(["run", "download", &run.run_id.to_string(), "-n", "test_workdir", "-D"])
        .arg(&dest);
    info!("> {cmd:?}");
    let status = cmd.status().context("Failed to spawn `gh run download`")?;
    if !status.success() {
        anyhow::bail!("`gh run download` exited with {status}");
    }
    info!("Downloaded artifact into {}", dest.display());
    Ok(())
}

fn gh_available() -> bool {
    Command::new("gh")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// One row of the run picker, derived from a single `test_workdir`
/// artifact entry returned by the GitHub artifacts API. Filtering at
/// the artifact level (rather than listing failed runs and hoping
/// they have an artifact) guarantees every entry is downloadable.
#[derive(Debug)]
struct ArtifactRun {
    run_id: u64,
    head_branch: String,
    head_sha: String,
    created_at: String,
}

impl ArtifactRun {
    fn label(&self) -> String {
        // 'createdAt' is RFC3339; first 16 chars = "YYYY-MM-DDTHH:MM".
        let created = self.created_at.get(..16).unwrap_or(&self.created_at);
        let short_sha = self.head_sha.get(..7).unwrap_or(&self.head_sha);
        format!(
            "{created} | {:<20} | {short_sha} [#{}]",
            truncate(&self.head_branch, 20),
            self.run_id,
        )
    }
}

#[derive(Debug, serde::Deserialize)]
struct ArtifactsResponse {
    artifacts: Vec<ArtifactEntry>,
}

#[derive(Debug, serde::Deserialize)]
struct ArtifactEntry {
    expired: bool,
    created_at: String,
    workflow_run: ArtifactWorkflowRun,
}

#[derive(Debug, serde::Deserialize)]
struct ArtifactWorkflowRun {
    id: u64,
    head_branch: String,
    head_sha: String,
}

/// Returns the most recent `test_workdir` artifacts attached to runs
/// of this repo, newest first. Hits the artifacts API directly with a
/// name filter so we never surface a run whose artifact doesn't
/// actually exist (or has expired retention).
fn list_recent_runs() -> Result<Vec<ArtifactRun>> {
    let mut cmd = Command::new("gh");
    cmd.args([
        "api",
        "-X",
        "GET",
        "repos/{owner}/{repo}/actions/artifacts",
        "-F",
        "name=test_workdir",
        "-F",
        "per_page=30",
    ]);
    let output = cmd.output().context("Failed to spawn `gh api`")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("`gh api` exited with {}: {stderr}", output.status);
    }
    let response: ArtifactsResponse = serde_json::from_slice(&output.stdout)
        .context("Failed to parse `gh api` output")?;
    // The API already returns newest first; filter expired artifacts
    // last so the list order matches what the user sees on GitHub.
    let runs = response
        .artifacts
        .into_iter()
        .filter(|a| !a.expired)
        .map(|a| ArtifactRun {
            run_id: a.workflow_run.id,
            head_branch: a.workflow_run.head_branch,
            head_sha: a.workflow_run.head_sha,
            created_at: a.created_at,
        })
        .collect();
    Ok(runs)
}
