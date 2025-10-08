use anyhow::{Context, Result, bail};
use regex::Regex;
use reqwest::blocking::get;
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};
use tracing::{error, info, warn};

const FFMPEG_LIB_DIR: &str = "ffmpeg_lib";
const FFMPEG_DOWNLOAD_DIR: &str = "ffmpeg_download";

#[cfg(target_os = "macos")]
const FFMPEG_ARCHIVE_NAME: &str = "ffmpeg.tar.gz";

#[cfg(target_os = "linux")]
const FFMPEG_ARCHIVE_NAME: &str = "ffmpeg.tar.xz";

/// FFMPEG_VERSION is set at compile time and contains FFmpeg version in `x.y` format which was used to compile
/// Smelter. FFmpeg version is found by matching `ffmpeg -version` output during compilation.
fn required_ffmpeg_version() -> &'static str {
    #[allow(clippy::option_env_unwrap)]
    option_env!("FFMPEG_VERSION").unwrap()
}

/// FFMPEG_URL is set at compile time and provides the URL of the FFmpeg prebuilt release with
/// libav dynamic libraries that should be downloaded if the required FFmpeg version is not installed
fn ffmpeg_url() -> &'static str {
    #[allow(clippy::option_env_unwrap)]
    option_env!("FFMPEG_URL").unwrap()
}

fn main() -> Result<()> {
    tracing_subscriber::fmt().init();
    // Matches if version is in correct format i.e. `x.y`
    let re = Regex::new(r"^\d+\.\d+$")?;
    if !re.is_match(required_ffmpeg_version()) {
        // NOTE: This should NEVER happen, especially that analogical check is performed at compile
        // time
        bail!("Version in invalid format: {}", required_ffmpeg_version());
    }

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

    let ffmpeg_installed = check_ffmpeg();
    let fetch_result = match ffmpeg_installed {
        Ok(true) => return Ok(()),
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
    cleanup(&executable_dir);

    fetch_result
}

fn cleanup(executable_dir: &Path) {
    let ffmpeg_archive = executable_dir.join(FFMPEG_ARCHIVE_NAME);
    let ffmpeg_download_dir = executable_dir.join(FFMPEG_DOWNLOAD_DIR);
    if let Err(error) = fs::remove_file(executable_dir.join(FFMPEG_ARCHIVE_NAME)) {
        error!(%error, "Failed to delete downloaded archive at {ffmpeg_archive:?}.");
    }
    if let Err(error) = fs::remove_dir_all(executable_dir.join(FFMPEG_DOWNLOAD_DIR)) {
        error!(%error, "Failed to delete downloaded archive at {ffmpeg_download_dir:?}.");
    }
}

fn prepare_dependencies(executable_dir: &Path) -> Result<()> {
    download_ffmpeg(executable_dir).with_context(|| "libav download failed.")?;

    let ffmpeg_archive_path = executable_dir.join(FFMPEG_ARCHIVE_NAME);
    let ffmpeg_download_dir = executable_dir.join(FFMPEG_DOWNLOAD_DIR);

    let tar_compression = if cfg!(target_os = "macos") {
        "--gzip"
    } else if cfg!(target_os = "linux") {
        "--xz"
    } else {
        panic!("Unknown platform");
    };

    fs::create_dir(&ffmpeg_download_dir)
        .with_context(|| format!("Failed to create directory {ffmpeg_download_dir:?}",))?;

    let tar_code = Command::new("tar")
        .args([
            tar_compression,
            "-xf",
            ffmpeg_archive_path.to_str().unwrap_or(FFMPEG_ARCHIVE_NAME),
            "-C",
            ffmpeg_download_dir
                .to_str()
                .expect("Failed to convert directory path to string"),
        ])
        .spawn()
        .with_context(|| "Failed to spawn `tar` process")?
        .wait()
        .with_context(|| "`tar` process failed")?
        .code();
    match tar_code {
        Some(0) => {}
        Some(code) => bail!("`tar` command failed with code: {code}."),
        None => bail!("`tar` command failed."),
    }

    let re = if cfg!(target_os = "linux") {
        // Matches dynamic libav library with major version only on linux
        // E.g. libavcodec.so.62
        Regex::new(r"^lib[a-zA-Z]+\.so\.\d+$")?
    } else if cfg!(target_os = "macos") {
        // Matches dynamic libav library with major version only on macos
        // E.g. libavcodec.62.dylib
        Regex::new(r"^lib[a-zA-Z]+\.\d+\.dylib$")?
    } else {
        panic!("Unknown platform");
    };

    let ffmpeg_libs_paths = extract_libav_libraries(executable_dir.join(FFMPEG_DOWNLOAD_DIR), &re)
        .with_context(|| "Failed to extract libav libraries from downloaded archive.")?;
    fs::create_dir(executable_dir.join(FFMPEG_LIB_DIR))?;
    for lib in ffmpeg_libs_paths {
        let libname = match lib.file_name() {
            Some(name) => name.to_owned(),
            None => {
                error!("Unable to get library filename");
                continue;
            }
        };
        fs::rename(lib, executable_dir.join(FFMPEG_LIB_DIR).join(libname))?;
    }

    Ok(())
}

// Recursively scans the downloaded archive for libav dynamic libraries —
// `*.n.dylib` on macOS and `*.so.n` on Linux, where `n` is the version number.
// If a found library is a symlink (as in prebuilt Linux libs), it follows the link,
// removes the symlink, and renames the target file.
//
// Returns a vector of paths to the libav files.
fn extract_libav_libraries(dirpath: PathBuf, re: &Regex) -> Result<Vec<PathBuf>> {
    let mut libraries: Vec<PathBuf> = vec![];
    for file in fs::read_dir(dirpath)?.flatten() {
        if file.file_type()?.is_dir() {
            let path = file.path();
            libraries.extend(extract_libav_libraries(path, re)?);
        } else if file.file_type()?.is_symlink() {
            let path = file.path();
            let filename = path
                .file_name()
                .unwrap_or_default()
                .to_str()
                .unwrap_or_default();
            if re.is_match(filename) {
                let target_file_path =
                    fs::read_link(&path).with_context(|| "Failed to read symlink")?;
                let target_file_path = if target_file_path.is_absolute() {
                    target_file_path
                } else {
                    let dir = match path.parent() {
                        Some(p) => p,
                        None => bail!("Failed to find parent directory of {path:?}"),
                    };
                    dir.join(target_file_path)
                };
                fs::remove_file(&path).with_context(|| "Symlink removal failed")?;
                fs::rename(target_file_path, &path).with_context(|| "Failed to rename file")?;
                libraries.push(path);
            }
        } else if file.file_type()?.is_file() {
            let path = file.path();
            let filename = path
                .file_name()
                .unwrap_or_default()
                .to_str()
                .unwrap_or_default();
            if re.is_match(filename) {
                libraries.push(path);
            }
        } else {
            unreachable!();
        }
    }
    Ok(libraries)
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
