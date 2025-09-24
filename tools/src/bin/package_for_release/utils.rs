use anyhow::{anyhow, Result};
use log::{info, warn};
use regex::Regex;
use std::{fs, path::PathBuf, process::Command, str::from_utf8};
use tools::paths::git_root;

pub enum SmelterBin {
    DependencyCheck,
    MainProcess,
    ChromiumHelper,
}

impl SmelterBin {
    fn bin_name(&self) -> &'static str {
        match self {
            SmelterBin::DependencyCheck => "dependency_check",
            SmelterBin::MainProcess => "main_process",
            SmelterBin::ChromiumHelper => "process_helper",
        }
    }
}

pub fn compile_smelter(
    bin: SmelterBin,
    target: &'static str,
    disable_default_features: bool,
    rustc_args: Option<&[String]>,
    rustc_envs: Option<Vec<(&'static str, String)>>,
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

    let rustc_args = rustc_args.unwrap_or_default();
    let rustc_envs = rustc_envs.unwrap_or_default();

    info!("Running command \"cargo {}\"", args.join(" "));
    let output = Command::new("cargo")
        .envs(rustc_envs)
        .args(args)
        .arg("--")
        .args(rustc_args)
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

pub fn ffmpeg_version() -> Result<u8> {
    let ffmpeg_output = Command::new("ffmpeg").arg("-version").output()?;
    let ffmpeg_output = String::from_utf8(ffmpeg_output.stdout)?.trim().to_string();

    let re = Regex::new(r"(?m)^ffmpeg version \D*(\d+).\S+")?;

    let caps = re.captures(&ffmpeg_output).unwrap();
    let version_str = caps.get(1).unwrap().as_str();
    let version = version_str.parse::<u8>()?;
    Ok(version)
}
