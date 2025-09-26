use anyhow::{bail, Context, Result};
use regex::Regex;
use reqwest::blocking::get;
use std::{
    env,
    fs::{self, File},
    io::{ErrorKind, Write},
    path::Path,
    process::Command,
};
use tracing::{error, info, warn};

const FFMPEG_LIB_DIR: &str = "ffmpeg_lib";
const FFMPEG_DOWNLOAD_DIR: &str = "ffmpeg_download";
const FFMPEG_REQUIRED_VERSION: &str = env!("FFMPEG_VERSION");
const FFMPEG_URL: &str = env!("FFMPEG_URL");

#[cfg(target_os = "macos")]
const FFMPEG_ARCHIVE_NAME: &str = "ffmpeg.tar.gz";

#[cfg(target_os = "linux")]
const FFMPEG_ARCHIVE_NAME: &str = "ffmpeg.tar.xz";

fn main() -> Result<()> {
    tracing_subscriber::fmt().init();

    let executable_path =
        env::current_exe().with_context(|| "Failed to get current executable directory.")?;
    let executable_dir = executable_path.parent();
    let executable_dir = match executable_dir {
        Some(path) => path.to_path_buf(),
        None => bail!("Failed to get current executable directory."),
    };

    let lib_exists = fs::exists(executable_dir.join(FFMPEG_LIB_DIR))
        .with_context(|| "Failed to check if local ffmpeg lib directory exists.")?;
    if lib_exists {
        return Ok(());
    }

    let ffmpeg_installed = check_ffmpeg(FFMPEG_REQUIRED_VERSION);
    let fetch_result = match ffmpeg_installed {
        Ok(true) => Ok(()),
        Ok(false) => {
            info!("Downloading dependencies...");
            prepare_dependencies(&executable_dir).with_context(|| "Failed to fetch dependencies.")
        }
        Err(error) => {
            error!(%error);
            info!("Downloading dependencies...");
            prepare_dependencies(&executable_dir).with_context(|| "Failed to fetch dependencies.")
        }
    };
    if let Err(e) = cleanup(&executable_dir) {
        if e.kind() != ErrorKind::NotFound {
            error!(error = %e, "Failed to remove unnecessary files");
            return Err(e.into());
        }
    }

    fetch_result
}

fn download_ffmpeg(executable_dir: &Path) -> Result<()> {
    let response = get(FFMPEG_URL)?;
    let content = response.bytes()?;

    let mut downloaded_libs = File::create(executable_dir.join(FFMPEG_ARCHIVE_NAME))?;
    downloaded_libs.write_all(&content)?;
    Ok(())
}

#[cfg(target_os = "linux")]
fn check_ffmpeg(required_ffmpeg_version: &str) -> Result<bool> {
    check_ffmpeg_command(required_ffmpeg_version)
}

#[cfg(target_os = "macos")]
fn check_ffmpeg(required_ffmpeg_version: &str) -> Result<bool> {
    let command_result = check_ffmpeg_command(required_ffmpeg_version)?;
    if !command_result {
        info!("Checking if ffmpeg is installed as homebrew keg-only");
        return check_ffmpeg_homebrew(required_ffmpeg_version);
    }
    Ok(command_result)
}

fn prepare_dependencies(executable_dir: &Path) -> Result<()> {
    download_ffmpeg(executable_dir)?;

    let ffmpeg_archive_path = executable_dir.join(FFMPEG_ARCHIVE_NAME);

    let tar_compression = if cfg!(target_os = "macos") {
        "--gzip"
    } else if cfg!(target_os = "linux") {
        "--xz"
    } else {
        bail!("Invalid platform");
    };

    fs::create_dir(executable_dir.join(FFMPEG_DOWNLOAD_DIR))
        .with_context(|| "Failed to create directory")?;

    let tar_code = Command::new("tar")
        .args([
            tar_compression,
            "-xf",
            ffmpeg_archive_path.to_str().unwrap_or(FFMPEG_ARCHIVE_NAME),
            "-C",
            executable_dir.join(FFMPEG_DOWNLOAD_DIR).to_str().unwrap(),
        ])
        .spawn()?
        .wait()?
        .code();
    if tar_code != Some(0) {
        bail!("\"tar\" command failed with code: {tar_code:?}");
    }

    let re = Regex::new(r"^ffmpeg.*")?;
    for file in fs::read_dir(executable_dir.join(FFMPEG_DOWNLOAD_DIR))?.flatten() {
        if file.file_type()?.is_dir() {
            let filename = file.file_name().into_string();
            let filename = match filename {
                Ok(f) => f,
                Err(_) => {
                    error!("Failed to parse ffmpeg directory name");
                    continue;
                }
            };
            if re.is_match(&filename) {
                fs::rename(
                    executable_dir.join(format!("{FFMPEG_DOWNLOAD_DIR}/{filename}/lib")),
                    executable_dir.join(FFMPEG_LIB_DIR),
                )
                .with_context(|| "Failed to move libraries to executable path")?;
                break;
            }
        }
    }

    Ok(())
}

fn check_ffmpeg_command(required_ffmpeg_version: &str) -> Result<bool> {
    let ffmpeg_result = Command::new("ffmpeg").arg("-version").output();
    match ffmpeg_result {
        Ok(ffmpeg_output) => {
            let ffmpeg_output = String::from_utf8(ffmpeg_output.stdout)?.trim().to_string();
            let re = Regex::new(r"(?m)^ffmpeg version \D*(\d+\.\d+)")?;
            let caps = re.captures(&ffmpeg_output);
            match caps {
                Some(caps) => {
                    let version = caps.get(1).unwrap().as_str();
                    if version == required_ffmpeg_version {
                        Ok(true)
                    } else {
                        warn!(
                            installed_ffmpeg_version = version,
                            required_ffmpeg_version,
                            "Inatelled version doesn't match the required version."
                        );
                        Ok(false)
                    }
                }
                None => {
                    warn!("Failed to parse FFmpeg version.");
                    Ok(false)
                }
            }
        }
        Err(_) => {
            warn!("Failed to run FFmpeg.");
            Ok(false)
        }
    }
}

#[cfg(target_os = "macos")]
fn check_ffmpeg_homebrew(required_ffmpeg_version: &str) -> Result<bool> {
    let required_ffmpeg_version_brew =
        &required_ffmpeg_version[..required_ffmpeg_version.find(".").unwrap_or(1)];
    let brew_output = Command::new("brew").arg("list").output()?;
    let brew_output = String::from_utf8(brew_output.stdout)?.trim().to_string();

    let ffmpeg_string = format!("ffmpeg@{required_ffmpeg_version_brew}");

    let re = Regex::new(&format!("(?m){ffmpeg_string}"))?;
    if re.is_match(&brew_output) {
        Ok(true)
    } else {
        warn!("FFmpeg installation not found in homebrew");
        Ok(false)
    }
}

fn cleanup(executable_dir: &Path) -> std::io::Result<()> {
    fs::remove_file(executable_dir.join(FFMPEG_ARCHIVE_NAME))?;
    fs::remove_dir_all(executable_dir.join(FFMPEG_DOWNLOAD_DIR))
}
