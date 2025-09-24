use anyhow::{anyhow, bail, Result};
use log::{info, warn};
use std::{fs, path::PathBuf, process::Command, str::from_utf8};
use tools::paths::git_root;

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
        "rustc",
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

    let rustc_args = if cfg!(target_os = "macos") {
        "-Clink-args=-Wl,-rpath,/opt/homebrew/opt/ffmpeg/lib -Wl,-rpath,/usr/local/lib -Wl,-rpath,@executable_path/ffmpeg_lib"
    } else if cfg!(target_os = "linux") {
        // TODO: (@jbrs) Add appropriate linker args for linux
        ""
    } else {
        bail!("Invalid platform");
    };
    println!("{rustc_args}");

    info!("Running command \"cargo {}\"", args.join(" "));
    let output = Command::new("cargo")
        .args(args)
        .arg("--")
        .arg(rustc_args)
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
