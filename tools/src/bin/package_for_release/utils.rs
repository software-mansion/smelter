use anyhow::{Result, anyhow};
use std::{fs, path::PathBuf, process::Command, str::from_utf8};
use tools::paths::git_root;
use tracing::{info, warn};

pub enum SmelterBin {
    MainProcess,
    ChromiumHelper,
}

impl SmelterBin {
    fn bin_name(&self) -> &'static str {
        match self {
            SmelterBin::MainProcess => "main_process",
            SmelterBin::ChromiumHelper => "process_helper",
        }
    }
}

pub fn compile_smelter(
    bin: SmelterBin,
    target: &'static str,
    disable_default_features: bool,
) -> Result<()> {
    let mut args = vec![
        "build",
        "--release",
        "--target",
        target,
        "--locked",
        "--bin",
        bin.bin_name(),
    ];
    if disable_default_features {
        args.extend(["--no-default-features"]);
    }

    info!("Running command \"cargo {}\"", args.join(" "));
    let output = Command::new("cargo")
        .args(args)
        .current_dir(git_root())
        .spawn()?
        .wait_with_output()?;
    if !output.status.success() {
        warn!("stdout: {:?}", &from_utf8(&output.stdout));
        warn!("stderr: {:?}", &from_utf8(&output.stderr));
        return Err(anyhow!("Command failed with exit code {}.", output.status));
    }
    Ok(())
}

pub fn ensure_empty_dir(dir: &PathBuf) -> Result<()> {
    if dir.exists() {
        if !dir.is_dir() {
            return Err(anyhow!("Expected directory path"));
        }

        info!("Bundle directory already exists. Removing...");
        fs::remove_dir_all(dir)?;
    }

    info!("Creating new bundle directory");
    fs::create_dir_all(dir)?;

    Ok(())
}
