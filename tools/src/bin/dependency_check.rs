use anyhow::{Context, Result, bail};
use regex::Regex;
use reqwest::blocking::get;
use std::{env, fs, path::Path, process::Command, sync::OnceLock};
use tracing::{error, info, warn};

const FFMPEG_LIB_DIR: &str = "libav";
const FFMPEG_ARCHIVE_NAME: &str = "ffmpeg.tar.gz";

/// FFMPEG_VERSION is set at compile time (in package_for_release bin) and contains FFmpeg version in `x.y` format which was used to compile
/// Smelter. FFmpeg version is found by matching `ffmpeg -version` output during compilation.
fn required_ffmpeg_version() -> &'static str {
    static VERSION: OnceLock<&'static str> = OnceLock::new();
    VERSION.get_or_init(|| {
        #[allow(clippy::option_env_unwrap)]
        let version = option_env!("FFMPEG_VERSION").unwrap();
        // Matches if version is in correct format i.e. `x.y`
        let re = Regex::new(r"^\d+\.\d+$").expect("Failed to compile RegEx");
        if !re.is_match(version) {
            // NOTE: This should NEVER happen, especially that analogical check is performed at compile
            // time
            panic!("Version in invalid format: {}", required_ffmpeg_version());
        }
        version
    })
}

/// FFMPEG_URL is set at compile time (in package_for_release bin) and provides the URL of the FFmpeg prebuilt release with
/// libav dynamic libraries that should be downloaded if the required FFmpeg version is not installed
fn ffmpeg_url() -> &'static str {
    #[allow(clippy::option_env_unwrap)]
    option_env!("FFMPEG_URL").unwrap()
}

fn main() -> Result<()> {
    tracing_subscriber::fmt().init();

    let executable_path =
        env::current_exe().with_context(|| "Failed to get current executable directory.")?;
    let executable_dir = executable_path.parent();
    let executable_dir = match executable_dir {
        Some(path) => path.to_path_buf(),
        None => bail!("Failed to get current executable directory."),
    };

    let lib_exists = fs::exists(executable_dir.join(FFMPEG_LIB_DIR).join(".ready"))
        .with_context(|| "Failed to check if local ffmpeg lib directory exists.")?;
    if lib_exists {
        return Ok(());
    } else {
        let malformed_lib_exists = fs::exists(executable_dir.join(FFMPEG_LIB_DIR))?;
        if malformed_lib_exists {
            fs::remove_dir_all(executable_dir.join(FFMPEG_LIB_DIR))?;
        }
    }

    let ffmpeg_installed = check_ffmpeg();
    match ffmpeg_installed {
        Ok(true) => return Ok(()),
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
    cleanup(&executable_dir);

    fs::write(executable_dir.join(FFMPEG_LIB_DIR).join(".ready"), "")?;
    Ok(())
}

fn cleanup(executable_dir: &Path) {
    let ffmpeg_archive = executable_dir.join(FFMPEG_ARCHIVE_NAME);
    if let Err(error) = fs::remove_file(executable_dir.join(FFMPEG_ARCHIVE_NAME)) {
        error!(%error, "Failed to delete downloaded archive at {ffmpeg_archive:?}.");
    }
}

fn prepare_dependencies(executable_dir: &Path) -> Result<()> {
    download_ffmpeg(executable_dir).with_context(|| "libav download failed.")?;

    let ffmpeg_archive_path = executable_dir.join(FFMPEG_ARCHIVE_NAME);

    // Archive contains only `libav` directory in the root. This directory contains all smelter
    // libav dependencies.
    let tar_status = Command::new("tar")
        .args([
            "-zxf",
            ffmpeg_archive_path
                .to_str()
                .expect("Unable to resolve executable directory as string"),
            "-C",
            executable_dir
                .to_str()
                .expect("Unable to resolve executable directory as string"),
        ])
        .status();
    match tar_status {
        Ok(status) => match status.code() {
            Some(0) => {}
            Some(code) => bail!("`tar` command failed with code: {code}"),
            None => bail!("`tar` command failed"),
        },
        Err(error) => return Err(anyhow::Error::from(error).context("`tar` command failed")),
    }

    Ok(())
}

fn download_ffmpeg(executable_dir: &Path) -> Result<()> {
    let response = get(ffmpeg_url())
        .with_context(|| format!("Failed to download libav libraries. URL: {}", ffmpeg_url()))?;
    let content = response.bytes()?;

    fs::write(executable_dir.join(FFMPEG_ARCHIVE_NAME), &content)
        .with_context(|| "Failed to save downloaded libav libraries to file")?;
    Ok(())
}

#[cfg(target_os = "linux")]
fn check_ffmpeg() -> Result<bool> {
    check_ffmpeg_command()
}

#[cfg(target_os = "macos")]
fn check_ffmpeg() -> Result<bool> {
    let command_result = check_ffmpeg_command()?;
    if !command_result {
        info!("Checking if FFmpeg is installed as homebrew keg-only");
        return check_ffmpeg_homebrew();
    }
    Ok(command_result)
}

fn check_ffmpeg_command() -> Result<bool> {
    let ffmpeg_result = Command::new("ffmpeg").arg("-version").output();
    match ffmpeg_result {
        Ok(ffmpeg_output) => {
            let ffmpeg_output = String::from_utf8(ffmpeg_output.stdout)?.trim().to_string();
            match match_ffmpeg_version(&ffmpeg_output) {
                Some(version) if version == required_ffmpeg_version() => Ok(true),
                Some(version) => {
                    warn!(
                        installed_ffmpeg_version = version,
                        required_ffmpeg_version = required_ffmpeg_version(),
                        "Installed version doesn't match the required version."
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
    // Matches ffmpeg version in the output of `ffmpeg -version` command, then returns the version.
    // E.g. "ffmpeg version 8.0" (captures 8.0) or "ffmpeg version n7.1" (captures 7.1)
    let re = Regex::new(r"(?m)^ffmpeg version \D*(\d+\.\d+)")
        .expect("Failed to compile regular expression");
    let caps = re.captures(ffmpeg_output);
    match caps {
        Some(caps) => caps.get(1).map(|cap| cap.as_str()),
        None => {
            warn!("Failed to parse FFmpeg version.");
            None
        }
    }
}

#[cfg(target_os = "macos")]
fn check_ffmpeg_homebrew() -> Result<bool> {
    let required_ffmpeg_version_brew =
        &required_ffmpeg_version()[..required_ffmpeg_version().find(".").unwrap_or(1)];
    let brew_output = Command::new("brew").arg("list").output()?;
    let brew_output = String::from_utf8(brew_output.stdout)?.trim().to_string();

    let re = Regex::new(&format!("(?m)ffmpeg@{required_ffmpeg_version_brew}"))?;
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

    #[test]
    fn ffmpeg_regex_test() {
        const FFMPEG_OUTPUT_MAC: &str =
            "ffmpeg version 8.0 Copyright (c) 2000-2025 the FFmpeg developers
built with Apple clang version 17.0.0 (clang-1700.0.13.3)";

        let actual_version_mac = match_ffmpeg_version(FFMPEG_OUTPUT_MAC).unwrap();
        assert_eq!(actual_version_mac, "8.0");

        const FFMPEG_OUTPUT_LINUX: &str =
            "ffmpeg version n7.1.1 Copyright (c) 2000-2025 the FFmpeg developers
built with gcc 15.1.1 (GCC) 20250425";

        let actual_version_linux = match_ffmpeg_version(FFMPEG_OUTPUT_LINUX).unwrap();
        assert_eq!(actual_version_linux, "7.1");
    }
}
