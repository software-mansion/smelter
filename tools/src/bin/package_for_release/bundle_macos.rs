use regex::Regex;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tools::paths::{git_root, tools_root};

use anyhow::{Result, bail};
use tracing::info;

use crate::utils::{self, SmelterBin};

const ARM_MAC_TARGET: &str = "aarch64-apple-darwin";
const ARM_OUTPUT_FILE: &str = "smelter_darwin_aarch64.tar.gz";
const ARM_WITH_WEB_RENDERER_OUTPUT_FILE: &str = "smelter_with_web_renderer_darwin_aarch64.tar.gz";

const INTEL_MAC_TARGET: &str = "x86_64-apple-darwin";
const INTEL_OUTPUT_FILE: &str = "smelter_darwin_x86_64.tar.gz";
const INTEL_WITH_WEB_RENDERER_OUTPUT_FILE: &str = "smelter_with_web_renderer_darwin_x86_64.tar.gz";

pub fn bundle_macos_app() -> Result<()> {
    tracing_subscriber::fmt().init();

    let workdir = tools_root().join("build");
    utils::ensure_empty_dir(&workdir)?;

    if cfg!(target_arch = "x86_64") {
        bundle_app_no_ffmpeg(&workdir, INTEL_MAC_TARGET, INTEL_OUTPUT_FILE, false)?;
        bundle_app_no_ffmpeg(
            &workdir,
            INTEL_MAC_TARGET,
            INTEL_WITH_WEB_RENDERER_OUTPUT_FILE,
            true,
        )?;
    } else if cfg!(target_arch = "aarch64") {
        bundle_app_with_ffmpeg(&workdir, ARM_MAC_TARGET, ARM_OUTPUT_FILE, false)?;
        bundle_app_with_ffmpeg(
            &workdir,
            ARM_MAC_TARGET,
            ARM_WITH_WEB_RENDERER_OUTPUT_FILE,
            true,
        )?;
    } else {
        panic!("Unknown architecture");
    }
    Ok(())
}

fn bundle_app_with_ffmpeg(
    workdir: &Path,
    target: &'static str,
    output_name: &str,
    enable_web_rendering: bool,
) -> Result<()> {
    if enable_web_rendering {
        info!("Bundling smelter with web rendering.");
    } else {
        info!("Bundling smelter without web rendering.");
    }

    let ffmpeg_version = utils::ffmpeg_version()?;
    // Matches if version is in correct format i.e. `x.y`
    let re = Regex::new(r"^\d+\.\d+$")?;
    if !re.is_match(&ffmpeg_version) {
        bail!("Version in invalid format: {ffmpeg_version}");
    }

    let ffmpeg_version_homebrew = &ffmpeg_version[..ffmpeg_version.find(".").unwrap_or(1)];
    let ffmpeg_url = utils::ffmpeg_url(&ffmpeg_version)?;
    let brew_prefix = Command::new("brew").arg("--prefix").output()?;
    let brew_prefix = String::from_utf8(brew_prefix.stdout)?.trim().to_string();

    let rustc_args = [
        format!("-Clink-arg=-Wl,-rpath,{brew_prefix}/opt/ffmpeg/lib"),
        format!("-Clink-arg=-Wl,-rpath,{brew_prefix}/opt/ffmpeg@{ffmpeg_version_homebrew}/lib"),
        "-Clink-arg=-Wl,-rpath,/usr/local/lib".to_string(),
        "-Clink-arg=-Wl,-rpath,@executable_path/libav".to_string(),
        format!("-Clink-arg=-Wl,-rpath,{brew_prefix}/lib"),
    ];

    let rustc_envs = vec![
        ("FFMPEG_VERSION", ffmpeg_version),
        ("FFMPEG_URL", ffmpeg_url),
    ];

    let cargo_build_dir = git_root().join("target").join(target).join("release");
    utils::ensure_empty_dir(&workdir.join("smelter"))?;

    info!("Build main_process binary.");
    utils::compile_smelter(
        SmelterBin::MainProcess,
        target,
        !enable_web_rendering,
        Some(&rustc_args),
        None,
    )?;

    info!("Build dependency_check binary.");
    utils::compile_smelter(
        SmelterBin::DependencyCheck,
        target,
        false,
        None,
        Some(rustc_envs),
    )?;
    fs::copy(
        cargo_build_dir.join("dependency_check"),
        workdir.join("smelter/dependency_check"),
    )?;

    info!("Create macOS bundle.");
    if enable_web_rendering {
        info!("Build process_helper binary.");
        utils::compile_smelter(SmelterBin::ChromiumHelper, target, false, None, None)?;
        libcef::bundle_app(&cargo_build_dir, &workdir.join("smelter/smelter.app"))?;
    }

    fs::copy(
        cargo_build_dir.join("main_process"),
        workdir.join("smelter/smelter_main"),
    )?;
    let smelter_bin_path = workdir.join("smelter/smelter_main");

    fs::copy(
        tools_root().join("src/bin/package_for_release/runtime_wrapper.sh"),
        workdir.join("smelter/smelter"),
    )?;

    let otool_output_bytes = Command::new("otool")
        .arg("-L")
        .arg(&smelter_bin_path)
        .output()?;
    let otool_output = String::from_utf8(otool_output_bytes.stdout)?;

    let brew_prefix_bytes = Command::new("brew").arg("--prefix").output()?;
    let brew_prefix = regex::escape(String::from_utf8(brew_prefix_bytes.stdout)?.trim());

    let re = Regex::new(&format!("(?m)({brew_prefix}\\S+|@loader_path\\S+)"))?;
    for (_, [path]) in re.captures_iter(&otool_output).map(|c| c.extract()) {
        let path_buf = PathBuf::from(path);
        let basename = path_buf.file_name().unwrap().to_str().unwrap();
        let exit_code = Command::new("install_name_tool")
            .args([
                "-change",
                path,
                &format!("@rpath/{basename}"),
                smelter_bin_path.to_str().unwrap(),
            ])
            .status()?
            .code();
        if exit_code != Some(0) {
            bail!("Command \"install_name_tool\" failed with exit code: {exit_code:?}");
        }
    }

    info!("Create tar.gz archive.");
    let exit_code = Command::new("tar")
        .args(["-czvf", output_name, "smelter"])
        .current_dir(workdir)
        .status()?
        .code();
    if exit_code != Some(0) {
        bail!("Command \"tar\" failed with exit code {:?}", exit_code);
    }

    Ok(())
}

fn bundle_app_no_ffmpeg(
    workdir: &Path,
    target: &'static str,
    output_name: &str,
    enable_web_rendering: bool,
) -> Result<()> {
    if enable_web_rendering {
        info!("Bundling smelter with web rendering.");
    } else {
        info!("Bundling smelter without web rendering.");
    }

    let cargo_build_dir = git_root().join("target").join(target).join("release");
    utils::ensure_empty_dir(&workdir.join("smelter"))?;

    info!("Build main_process binary.");
    utils::compile_smelter(
        SmelterBin::MainProcess,
        target,
        !enable_web_rendering,
        None,
        None,
    )?;

    info!("Create macOS bundle.");
    if enable_web_rendering {
        info!("Build process_helper binary.");
        utils::compile_smelter(SmelterBin::ChromiumHelper, target, false, None, None)?;
        libcef::bundle_app(&cargo_build_dir, &workdir.join("smelter/smelter.app"))?;
    }

    fs::copy(
        cargo_build_dir.join("main_process"),
        workdir.join("smelter/smelter"),
    )?;

    info!("Create tar.gz archive.");
    let exit_code = Command::new("tar")
        .args(["-czvf", output_name, "smelter"])
        .current_dir(workdir)
        .status()?
        .code();
    if exit_code != Some(0) {
        bail!("Command \"tar\" failed with exit code {:?}", exit_code);
    }

    Ok(())
}
