use anyhow::{anyhow, bail, Result};
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

pub fn ffmpeg_version() -> Result<String> {
    let ffmpeg_output = Command::new("ffmpeg").arg("-version").output()?;
    let ffmpeg_output = String::from_utf8(ffmpeg_output.stdout)?.trim().to_string();

    let re = Regex::new(r"(?m)^ffmpeg version \D*(\d+\.\d+)")?;

    let caps = re.captures(&ffmpeg_output).unwrap();
    let version = caps.get(1).unwrap().as_str();
    Ok(version.into())
}

pub fn ffmpeg_url(ffmpeg_version: &str) -> Result<String> {
    #[cfg(target_os = "linux")]
    const FFMPEG_URL_PREFIX: &str = "https://github.com/BtbN/FFmpeg-Builds/releases/download/";

    #[cfg(target_os = "macos")]
    // XXX: Temporary link to private repo, change it after it gets pushed to official one.
    const FFMPEG_URL_PREFIX: &str =
        "https://github.com/JBRS307/FFmpeg_macos_build/releases/download/";

    let ffmpeg_url_suffix = if cfg!(target_os = "linux") {
        let os_arch = if cfg!(target_arch = "x86_64") {
            "linux64"
        } else if cfg!(target_arch = "aarch64") {
            "linuxarm64"
        } else {
            bail!("Invalid architecture");
        };

        match ffmpeg_version {
            "6.0" => {
                format!("autobuild-2023-11-30-12-55/ffmpeg-n6.0.1-{os_arch}-lgpl-shared-6.0.tar.xz")
            }
            "6.1" => {
                format!("autobuild-2025-08-31-13-00/ffmpeg-n6.1.3-{os_arch}-lgpl-shared-6.1.tar.xz")
            }
            "7.0" => {
                format!("autobuild-2024-08-31-12-50/ffmpeg-n7.0.2-6-g7e69129d2f-{os_arch}-lgpl-shared-7.0.tar.xz")
            }
            "7.1" => {
                format!("autobuild-2025-09-25-15-12/ffmpeg-n7.1.2-4-g8320e6b415-{os_arch}-lgpl-shared-7.1.tar.xz")
            }
            "8.0" => {
                format!("autobuild-2025-09-25-15-12/ffmpeg-n8.0-16-gd8605a6b55-{os_arch}-lgpl-shared-8.0.tar.xz")
            }
            _ => bail!("Unsupported FFmpeg version"),
        }
    } else if cfg!(target_os = "macos") {
        let os_arch = if cfg!(target_arch = "x86_64") {
            bail!("Download not available for macos with amd64 architecture");
        } else if cfg!(target_arch = "aarch64") {
            "macos_arm"
        } else {
            bail!("Invalid architecture");
        };

        match ffmpeg_version {
            "8.0" => {
                format!("n8.0/ffmpeg_{os_arch}.tar.gz")
            }
            _ => bail!("Unsupported FFmpeg version"),
        }
    } else {
        panic!("Unknown platform");
    };

    Ok(FFMPEG_URL_PREFIX.to_string() + &ffmpeg_url_suffix)
}
