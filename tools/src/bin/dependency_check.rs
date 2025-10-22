use anyhow::{Context, Result, bail};
use regex::Regex;
use reqwest::blocking::get;
use std::{
    env,
    fs::{self, File},
    io::BufReader,
    path::Path,
    process::Command,
    sync::OnceLock,
};
use tar::Archive;
use tracing::{error, info, trace, warn};

const FFMPEG_LIB_DIR: &str = "libav";
const FFMPEG_GZIP_ARCHIVE_NAME: &str = "ffmpeg.tar.gz";
const FFMPEG_TAR_ARCHIVE_NAME: &str = "ffmpeg.tar";

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
    self::logger::init_logger();

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
        let gz_archive_exists = fs::exists(executable_dir.join(FFMPEG_GZIP_ARCHIVE_NAME))?;
        if gz_archive_exists {
            fs::remove_file(executable_dir.join(FFMPEG_GZIP_ARCHIVE_NAME))?;
        }
        let tar_archive_exists = fs::exists(executable_dir.join(FFMPEG_TAR_ARCHIVE_NAME))?;
        if tar_archive_exists {
            fs::remove_file(executable_dir.join(FFMPEG_TAR_ARCHIVE_NAME))?;
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
    let gz_archive = executable_dir.join(FFMPEG_GZIP_ARCHIVE_NAME);
    if let Err(error) = fs::remove_file(&gz_archive) {
        error!(%error, "Failed to delete downloaded archive at {gz_archive:?}.");
    }

    let tar_archive = executable_dir.join(FFMPEG_TAR_ARCHIVE_NAME);
    if let Err(error) = fs::remove_file(&tar_archive) {
        error!(%error, "Failed to delete downloaded archive at {tar_archive:?}.");
    }
}

fn prepare_dependencies(executable_dir: &Path) -> Result<()> {
    download_ffmpeg(executable_dir).with_context(|| "libav download failed.")?;

    // Archive contains only `libav` directory in the root. This directory contains all smelter
    // libav dependencies.
    unpack_ffmpeg(executable_dir).with_context(|| "Failed to unpack downloaded archive")?;

    Ok(())
}

fn download_ffmpeg(executable_dir: &Path) -> Result<()> {
    let response = get(ffmpeg_url())
        .with_context(|| format!("Failed to download libav libraries. URL: {}", ffmpeg_url()))?;
    let content = response.bytes()?;

    fs::write(executable_dir.join(FFMPEG_GZIP_ARCHIVE_NAME), &content)
        .with_context(|| "Failed to save downloaded libav libraries to file")?;
    Ok(())
}

fn unpack_ffmpeg(executable_dir: &Path) -> Result<()> {
    let gz_archive_path = executable_dir.join(FFMPEG_GZIP_ARCHIVE_NAME);
    let tar_archive_path = executable_dir.join(FFMPEG_TAR_ARCHIVE_NAME);

    let gz_input = BufReader::new(
        File::open(&gz_archive_path).with_context(|| "Failed to open downloaded archive")?,
    );
    let mut tar_output = File::create(&tar_archive_path)?;

    let mut decoder = flate2::bufread::GzDecoder::new(gz_input);
    let decompressed_bytes = std::io::copy(&mut decoder, &mut tar_output)
        .with_context(|| "Failed to decompress the archive")?;
    trace!(decompressed_bytes);

    drop(tar_output);
    let tar_output = File::open(&tar_archive_path).with_context(|| "Failed to open tar archive")?;

    let mut tar_archive = Archive::new(tar_output);
    tar_archive.unpack(executable_dir)?;

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
        return check_ffmpeg_homebrew()
            .with_context(|| "Failed to check homebrew FFmpeg installation");
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
    let brew_output = Command::new("brew").arg("list").output();
    let brew_output = match brew_output {
        Ok(output) => output,
        Err(error) => {
            error!(%error, "Unable to run `brew`.");
            warn!(
                "Downloaded libav libraries require the same dependencies as FFmpeg and will not work without them installed."
            );
            return Ok(false);
        }
    };
    let brew_output = String::from_utf8(brew_output.stdout)?.trim().to_string();

    let re = Regex::new(&format!("(?m)ffmpeg@{required_ffmpeg_version_brew}"))?;
    if re.is_match(&brew_output) {
        Ok(true)
    } else {
        warn!("FFmpeg installation not found in homebrew");
        warn!(
            "Downloaded libav libraries require the same dependencies as FFmpeg and will not work without them installed."
        );
        Ok(false)
    }
}

mod logger {
    use std::{env, str::FromStr};
    use tracing_subscriber::{Layer, Registry, fmt, layer::SubscriberExt, util::SubscriberInitExt};

    enum LoggerFormat {
        Pretty,
        Json,
        Compact,
    }

    impl FromStr for LoggerFormat {
        type Err = &'static str;

        fn from_str(s: &str) -> Result<Self, Self::Err> {
            match s {
                "json" => Ok(LoggerFormat::Json),
                "pretty" => Ok(LoggerFormat::Pretty),
                "compact" => Ok(LoggerFormat::Compact),
                _ => Err("invalid logger format"),
            }
        }
    }

    pub(super) fn init_logger() {
        let logger_level = match env::var("SMELTER_LOGGER_LEVEL") {
            Ok(level) => level,
            Err(_) => "info".to_string(),
        };
        let stdio_logger_level = match env::var("SMELTER_STDIO_LOGGER_LEVEL") {
            Ok(level) => level,
            Err(_) => logger_level.clone(),
        };
        let default_logger_format = LoggerFormat::Compact;
        let logger_format = match env::var("SMELTER_LOGGER_FORMAT") {
            Ok(format) => LoggerFormat::from_str(&format).unwrap_or(default_logger_format),
            Err(_) => default_logger_format,
        };

        let stdio_filter = tracing_subscriber::EnvFilter::new(stdio_logger_level);
        let stdio_layer = match logger_format {
            LoggerFormat::Pretty => fmt::Layer::default().pretty().boxed(),
            LoggerFormat::Json => fmt::Layer::default().json().boxed(),
            LoggerFormat::Compact => fmt::Layer::default().compact().boxed(),
        }
        .with_filter(stdio_filter);
        Registry::default().with(stdio_layer).init();
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
