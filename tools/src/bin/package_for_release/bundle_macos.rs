use std::fs;
use std::process::Command;
use tools::paths::{git_root, tools_root};

use anyhow::{anyhow, Result};
use log::info;

use crate::utils;
use crate::utils::SmelterBin;

const ARM_MAC_TARGET: &str = "aarch64-apple-darwin";
const ARM_OUTPUT_FILE: &str = "smelter_darwin_aarch64.tar.gz";
const ARM_WITH_WEB_RENDERER_OUTPUT_FILE: &str = "smelter_with_web_renderer_darwin_aarch64.tar.gz";

const INTEL_MAC_TARGET: &str = "x86_64-apple-darwin";
const INTEL_OUTPUT_FILE: &str = "smelter_darwin_x86_64.tar.gz";
const INTEL_WITH_WEB_RENDERER_OUTPUT_FILE: &str = "smelter_with_web_renderer_darwin_x86_64.tar.gz";

pub fn bundle_macos_app() -> Result<()> {
    tracing_subscriber::fmt().init();

    if cfg!(target_arch = "x86_64") {
        bundle_app(INTEL_MAC_TARGET, INTEL_OUTPUT_FILE, false)?;
        bundle_app(INTEL_MAC_TARGET, INTEL_WITH_WEB_RENDERER_OUTPUT_FILE, true)?;
    } else if cfg!(target_arch = "aarch64") {
        bundle_app(ARM_MAC_TARGET, ARM_OUTPUT_FILE, false)?;
        bundle_app(ARM_MAC_TARGET, ARM_WITH_WEB_RENDERER_OUTPUT_FILE, true)?;
    } else {
        panic!("Unknown architecture")
    }
    Ok(())
}

fn bundle_app(target: &'static str, output_name: &str, enable_web_rendering: bool) -> Result<()> {
    if enable_web_rendering {
        info!("Bundling smelter with web rendering.");
    } else {
        info!("Bundling smelter without web rendering.");
    }

    let cargo_build_dir = git_root().join("target").join(target).join("release");
    let workdir = tools_root().join("build");
    utils::ensure_empty_dir(&workdir)?;
    fs::create_dir_all(workdir.join("smelter"))?;

    info!("Build main_process binary.");
    utils::compile_smelter(SmelterBin::MainProcess, target, !enable_web_rendering)?;

    info!("Create macOS bundle.");
    #[cfg(feature = "web_renderer")]
    if enable_web_rendering {
        use compositor_chromium::cef;

        info!("Build process_helper binary.");
        utils::compile_smelter(SmelterBin::ChromiumHelper, target, false)?;
        cef::bundle_app(&cargo_build_dir, &workdir.join("smelter/smelter.app"))?;
    }

    fs::copy(
        cargo_build_dir.join("main_process"),
        workdir.join("smelter/smelter"),
    )?;

    info!("Create tar.gz archive.");
    let exit_code = Command::new("tar")
        .args(["-czvf", output_name, "smelter"])
        .current_dir(workdir)
        .spawn()?
        .wait()?
        .code();
    if exit_code != Some(0) {
        return Err(anyhow!("Command tar failed with exit code {:?}", exit_code));
    }

    Ok(())
}
