use anyhow::{Result, anyhow};
use fs_extra::dir::{self, CopyOptions};
use std::process::Command;
use std::{fs, path::Path};
use tools::paths::{git_root, tools_root};
use tracing::info;

use crate::utils::{self, SmelterBin};

const X86_TARGET: &str = "x86_64-unknown-linux-gnu";
const X86_OUTPUT_FILE: &str = "smelter_linux_x86_64.tar.gz";
const X86_WITH_WEB_RENDERER_OUTPUT_FILE: &str = "smelter_with_web_renderer_linux_x86_64.tar.gz";

const ARM_TARGET: &str = "aarch64-unknown-linux-gnu";
const ARM_OUTPUT_FILE: &str = "smelter_linux_aarch64.tar.gz";

pub fn bundle_linux_app() -> Result<()> {
    tracing_subscriber::fmt().init();

    let workdir = tools_root().join("build");
    utils::ensure_empty_dir(&workdir)?;

    if cfg!(target_arch = "x86_64") {
        bundle_app(&workdir, X86_TARGET, X86_OUTPUT_FILE, false)?;
        bundle_app(
            &workdir,
            X86_TARGET,
            X86_WITH_WEB_RENDERER_OUTPUT_FILE,
            true,
        )?;
    } else if cfg!(target_arch = "aarch64") {
        bundle_app(&workdir, ARM_TARGET, ARM_OUTPUT_FILE, false)?;
    }
    Ok(())
}

fn bundle_app(
    workdir: &Path,
    target_name: &'static str,
    output_name: &str,
    enable_web_rendering: bool,
) -> Result<()> {
    if enable_web_rendering {
        info!("Bundling smelter with web rendering");
    } else {
        info!("Bundling smelter without web rendering");
    }

    let cargo_build_dir = git_root().join("target").join(target_name).join("release");
    utils::ensure_empty_dir(&workdir.join("smelter"))?;

    info!("Build main_process binary.");
    utils::compile_smelter(SmelterBin::MainProcess, target_name, !enable_web_rendering)?;

    if enable_web_rendering {
        info!("Build process_helper binary.");
        utils::compile_smelter(SmelterBin::ChromiumHelper, target_name, false)?;

        info!("Copy main_process binary.");
        fs::copy(
            cargo_build_dir.join("main_process"),
            workdir.join("smelter/smelter_main"),
        )?;

        info!("Copy process_helper binary.");
        fs::copy(
            cargo_build_dir.join("process_helper"),
            workdir.join("smelter/smelter_process_helper"),
        )?;

        info!("Copy wrapper script.");
        fs::copy(
            tools_root().join("src/bin/package_for_release/linux_runtime_wrapper.sh"),
            workdir.join("smelter/smelter"),
        )?;

        info!(
            "Copy lib directory. {:?} {:?}",
            cargo_build_dir.join("lib"),
            workdir.join("smelter/lib"),
        );

        dir::copy(
            cargo_build_dir.join("lib"),
            workdir.join("smelter"),
            &CopyOptions::default(),
        )?;
    } else {
        info!("Copy main_process binary.");
        fs::copy(
            cargo_build_dir.join("main_process"),
            workdir.join("smelter/smelter"),
        )?;
    }

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
