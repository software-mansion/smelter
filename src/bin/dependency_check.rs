use anyhow::{bail, Context, Result};
use regex::Regex;
use reqwest::blocking::get;
use std::{env, fs, io::ErrorKind, path::Path, process::Command};
use tracing::{error, info, warn};

const FFMPEG_LIB_DIR: &str = "ffmpeg_lib";
const FFMPEG_DOWNLOAD_DIR: &str = "ffmpeg_download";

#[cfg(target_os = "macos")]
const FFMPEG_ARCHIVE_NAME: &str = "ffmpeg.tar.gz";

#[cfg(target_os = "linux")]
const FFMPEG_ARCHIVE_NAME: &str = "ffmpeg.tar.xz";

fn main() -> Result<()> {
    tracing_subscriber::fmt().init();

    #[allow(clippy::option_env_unwrap)]
    let (required_ffmpeg_version, ffmpeg_url) = (
        option_env!("FFMPEG_VERSION").unwrap(),
        option_env!("FFMPEG_URL").unwrap(),
    );

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
    let fetch_result = match ffmpeg_installed {
        Ok(true) => Ok(()),
        Ok(false) => {
            info!("Downloading dependencies...");
            prepare_dependencies(&executable_dir, ffmpeg_url)
                .with_context(|| "Failed to fetch dependencies.")
        }
        Err(error) => {
            error!(%error);
            info!("Downloading dependencies...");
            prepare_dependencies(&executable_dir, ffmpeg_url)
                .with_context(|| "Failed to fetch dependencies.")
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

fn cleanup(executable_dir: &Path) -> std::io::Result<()> {
    fs::remove_file(executable_dir.join(FFMPEG_ARCHIVE_NAME))?;
    fs::remove_dir_all(executable_dir.join(FFMPEG_DOWNLOAD_DIR))
}

fn prepare_dependencies(executable_dir: &Path, ffmpeg_url: &str) -> Result<()> {
    download_ffmpeg(executable_dir, ffmpeg_url)?;

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

fn download_ffmpeg(executable_dir: &Path, ffmpeg_url: &str) -> Result<()> {
    let response = get(ffmpeg_url)?;
    let content = response.bytes()?;

    fs::write(executable_dir.join(FFMPEG_ARCHIVE_NAME), &content)?;
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

fn check_ffmpeg_command(required_ffmpeg_version: &str) -> Result<bool> {
    let ffmpeg_result = Command::new("ffmpeg").arg("-version").output();
    match ffmpeg_result {
        Ok(ffmpeg_output) => {
            let ffmpeg_output = String::from_utf8(ffmpeg_output.stdout)?.trim().to_string();
            match match_ffmpeg_version(&ffmpeg_output) {
                Some(version) if version == required_ffmpeg_version => Ok(true),
                Some(version) => {
                    warn!(
                        installed_ffmpeg_version = version,
                        required_ffmpeg_version,
                        "Inatelled version doesn't match the required version."
                    );
                    Ok(false)
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

fn match_ffmpeg_version(ffmpeg_output: &str) -> Option<&str> {
    let re = Regex::new(r"(?m)^ffmpeg version \D*(\d+\.\d+)")
        .expect("Failed to compile regular expression");
    let caps = re.captures(&ffmpeg_output);
    match caps {
        Some(caps) => caps.get(1).map(|cap| cap.as_str()),
        None => {
            warn!("Failed to parse FFmpeg version.");
            None
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

#[cfg(test)]
mod dependency_check_test {
    use super::*;
    use std::{fs, path::PathBuf};

    // TODO: (@jbrs) test for linux output
    #[test]
    fn ffmpeg_macos_regex_test() {
        let crate_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let ffmpeg_output =
            fs::read_to_string(crate_root.join("src/bin/ffmpeg_output_macos.txt")).unwrap();

        let actual_version = match_ffmpeg_version(&ffmpeg_output).unwrap();

        assert_eq!(actual_version, "8.0");
    }
}
