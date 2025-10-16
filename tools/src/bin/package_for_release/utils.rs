use anyhow::{Result, anyhow, bail};
use regex::Regex;
use std::{fs, path::PathBuf, process::Command, str::from_utf8};
use tools::paths::git_root;
use tracing::{info, warn};

#[derive(PartialEq)]
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
        args.push("--no-default-features");
    }
    if bin == SmelterBin::DependencyCheck {
        args.extend(["-p", "tools"]);
    }

    let rustc_args = rustc_args.unwrap_or_default();
    let rustc_envs = rustc_envs.unwrap_or_default();

    info!(
        "Running command \"{} cargo {} -- {}\"",
        display_rustc_envs(&rustc_envs).join(" "),
        args.join(" "),
        rustc_args.join(" ")
    );
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

fn display_rustc_envs(rustc_envs: &Vec<(&'static str, String)>) -> Vec<String> {
    rustc_envs
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect()
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

    // Matches the FFmpeg version installed on machine and captures `x.y` where `x` is major and
    // `y` is minor.
    // E.g. "ffmpeg version n8.0" (captures 8.0)
    let re = Regex::new(r"(?m)^ffmpeg version \D*(\d+\.\d+)")?;

    let caps = re.captures(&ffmpeg_output).unwrap();
    let version = caps.get(1).unwrap().as_str();
    Ok(version.into())
}

pub fn ffmpeg_url(ffmpeg_version: &str) -> Result<String> {
    const FFMPEG_URL_PREFIX: &str =
        "https://github.com/smelter-labs/smelter-dep-prebuilds/releases/download/";

    let ffmpeg_url_suffix = if cfg!(target_os = "linux") {
        let os_arch = if cfg!(target_arch = "x86_64") {
            "linux_x86_64"
        } else if cfg!(target_arch = "aarch64") {
            "linux_arm64"
        } else {
            bail!("Invalid architecture");
        };

        match ffmpeg_version {
            "6.1" => {
                format!("n6.1/ffmpeg_n6.1_{os_arch}.tar.gz")
            }
            "7.1" => {
                format!("n7.1/ffmpeg_n7.1_{os_arch}.tar.gz")
            }
            "8.0" => {
                format!("n8.0/ffmpeg_n8.0_{os_arch}.tar.gz")
            }
            _ => bail!("Unsupported FFmpeg version"),
        }
    } else if cfg!(target_os = "macos") {
        let os_arch = if cfg!(target_arch = "x86_64") {
            bail!("Download not available for macos with amd64 architecture");
        } else if cfg!(target_arch = "aarch64") {
            "macos_arm64"
        } else {
            bail!("Invalid architecture");
        };

        match ffmpeg_version {
            "8.0" => {
                format!("n8.0/ffmpeg_n8.0_{os_arch}.tar.gz")
            }
            _ => bail!("Unsupported FFmpeg version"),
        }
    } else {
        panic!("Unknown platform");
    };

    Ok(FFMPEG_URL_PREFIX.to_string() + &ffmpeg_url_suffix)
}
