use std::fs;

use anyhow::Result;
use inquire::{Confirm, InquireError};
use tracing::{error, info};

use crate::{pipeline, render};

/// Scan both committed snapshot trees and report any file that no
/// registered test would produce. After listing the orphans the user
/// can confirm a bulk delete — useful after renaming or removing a
/// test.
pub(crate) fn cleanup_orphan_snapshots() -> Result<()> {
    let render_orphans = render::find_orphan_render_snapshots()?;
    let pipeline_orphans = pipeline::find_orphan_pipeline_snapshots()?;

    if render_orphans.is_empty() && pipeline_orphans.is_empty() {
        info!("No orphan snapshots found");
        return Ok(());
    }

    println!();
    if !render_orphans.is_empty() {
        println!("Orphan render snapshots ({}):", render_orphans.len());
        for p in &render_orphans {
            println!("  {}", p.display());
        }
    }
    if !pipeline_orphans.is_empty() {
        if !render_orphans.is_empty() {
            println!();
        }
        println!("Orphan pipeline snapshots ({}):", pipeline_orphans.len());
        for p in &pipeline_orphans {
            println!("  {}", p.display());
        }
    }
    println!();

    let total = render_orphans.len() + pipeline_orphans.len();
    let confirm = match Confirm::new(&format!("Delete {total} orphan snapshot(s)?"))
        .with_default(false)
        .prompt()
    {
        Ok(b) => b,
        Err(InquireError::OperationCanceled | InquireError::OperationInterrupted) => return Ok(()),
        Err(e) => return Err(e.into()),
    };
    if !confirm {
        return Ok(());
    }

    let mut deleted = 0usize;
    for path in render_orphans.iter().chain(pipeline_orphans.iter()) {
        match fs::remove_file(path) {
            Ok(()) => deleted += 1,
            Err(e) => error!("Failed to remove {}: {e:#}", path.display()),
        }
    }
    info!("Deleted {deleted} orphan snapshot(s)");
    Ok(())
}
