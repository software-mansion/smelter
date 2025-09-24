use anyhow::{bail, Context, Result};
use regex::Regex;
use reqwest::blocking::get;
use std::{
    env,
    fs::{self, File},
    io::Write,
    path::Path,
    process::Command,
};
use tracing::{error, info, warn};

const FFMPEG_LIB_DIR: &str = "ffmpeg_lib";

#[cfg(target_os = "macos")]
const FFMPEG_URL: &str = "https://github.com/membraneframework-precompiled/precompiled_ffmpeg/releases/download/v8.0/ffmpeg_macos_arm.tar.gz";

#[cfg(target_os = "linux")]
const FFMPEG_URL: &str = "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-n7.1-latest-linux64-lgpl-shared-7.1.tar.xz";

#[cfg(target_os = "macos")]
const FFMPEG_ARCHIVE_NAME: &str = "ffmpeg.tar.gz";

#[cfg(target_os = "linux")]
const FFMPEG_ARCHIVE_NAME: &str = "ffmpeg.tar.xz";

fn main() -> Result<()> {
    tracing_subscriber::fmt().init();

    #[allow(clippy::option_env_unwrap)]
    let required_ffmpeg_version = option_env!("FFMPEG_VERSION")
        .unwrap()
        .parse::<u8>()
        .expect("Failed to parse FFmpeg version");

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

    let ffmpeg_installed = check_ffmpeg(required_ffmpeg_version);
    match ffmpeg_installed {
        Ok(true) => {}
        Ok(false) => {
            info!("Downloading dependencies...");
            prepare_dependencies(&executable_dir)
                .with_context(|| "Failed to fetch dependencies.")?;
        }
        Err(error) => {
            error!(%error);
            info!("Downloading dependencies...");
            prepare_dependencies(&executable_dir)
                .with_context(|| "Failed to fetch dependencies.")?;
        }
    }

    Ok(())
}

fn download_ffmpeg(executable_dir: &Path) -> Result<()> {
    let response = get(FFMPEG_URL)?;
    let content = response.bytes()?;

    let mut downloaded_libs = File::create(executable_dir.join(FFMPEG_ARCHIVE_NAME))?;
    downloaded_libs.write_all(&content)?;
    Ok(())
}

#[cfg(target_os = "linux")]
fn check_ffmpeg(required_ffmpeg_version: u8) -> Result<bool> {
    check_ffmpeg_command(required_ffmpeg_version)
}

#[cfg(target_os = "macos")]
fn check_ffmpeg(required_ffmpeg_version: u8) -> Result<bool> {
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

    let tar_code = Command::new("tar")
        .args([
            tar_compression,
            "-xf",
            ffmpeg_archive_path.to_str().unwrap_or(FFMPEG_ARCHIVE_NAME),
        ])
        .spawn()?
        .wait()?
        .code();
    if tar_code != Some(0) {
        bail!("\"tar\" command failed with code: {tar_code:?}");
    }

    fs::remove_file(executable_dir.join(FFMPEG_ARCHIVE_NAME))
        .with_context(|| "Failed to remove tar archive")?;

    let re = Regex::new(r"^ffmpeg.*")?;
    for file in fs::read_dir(executable_dir)?.flatten() {
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
                    executable_dir.join(format!("{filename}/lib")),
                    executable_dir.join(FFMPEG_LIB_DIR),
                )
                .with_context(|| "Failed to move libraries to executable path")?;
                fs::remove_dir_all(executable_dir.join(filename))
                    .with_context(|| "Failed to remove unnecessary files")?;
                break;
            }
        }
    }

    Ok(())
}

fn check_ffmpeg_command(required_ffmpeg_version: u8) -> Result<bool> {
    let ffmpeg_result = Command::new("ffmpeg").arg("-version").output();
    match ffmpeg_result {
        Ok(ffmpeg_output) => {
            let ffmpeg_output = String::from_utf8(ffmpeg_output.stdout)?.trim().to_string();
            let re = Regex::new(r"(?m)^ffmpeg version \D*(\d+)\.\S+")?;
            let caps = re.captures(&ffmpeg_output);
            match caps {
                Some(caps) => {
                    let version_str = caps.get(1).unwrap().as_str();
                    let version = version_str.parse::<u8>();
                    match version {
                        Ok(version) if version == required_ffmpeg_version => Ok(true),
                        Ok(version) => {
                            warn!("Installed FFmpeg version - {version} - does not match required version - {required_ffmpeg_version}.");
                            Ok(false)
                        }
                        Err(error) => {
                            warn!(%error, "Unable to parse FFmpeg version.");
                            Ok(false)
                        }
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
fn check_ffmpeg_homebrew(required_ffmpeg_version: u8) -> Result<bool> {
    let brew_output = Command::new("brew").arg("list").output()?;
    let brew_output = String::from_utf8(brew_output.stdout)?.trim().to_string();

    let ffmpeg_string = format!("ffmpeg@{required_ffmpeg_version}");

    let re = Regex::new(&format!("(?m){ffmpeg_string}"))?;
    Ok(re.is_match(&brew_output))
}
