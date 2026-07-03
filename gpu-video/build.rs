fn main() {
    #[cfg(feature = "transcoder")]
    build_transcoding_shader();
    #[cfg(all(feature = "quicksync", target_os = "linux"))]
    build_quicksync_bindings();

    cfg_aliases::cfg_aliases! {
        vulkan: {
            any(
                windows,
                all(
                    unix,
                    not(any(target_os = "macos", target_os = "ios", target_os = "emscripten"))
                )
            )
        },
    }
}

#[cfg(feature = "transcoder")]
fn build_transcoding_shader() {
    println!("cargo:rerun-if-changed=src/vulkan_transcoder/shader.wgsl");

    let mut front = naga::front::wgsl::Frontend::new();
    let parsed = front
        .parse(include_str!("src/vulkan_transcoder/shader.wgsl"))
        .unwrap();
    let mut validator = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    );
    validator
        .subgroup_stages(naga::valid::ShaderStages::COMPUTE)
        .subgroup_operations(naga::valid::SubgroupOperationSet::all());
    let module_info = validator.validate(&parsed).unwrap();
    let compiled = naga::back::spv::write_vec(
        &parsed,
        &module_info,
        &naga::back::spv::Options {
            lang_version: (1, 6),
            ..Default::default()
        },
        Some(&naga::back::spv::PipelineOptions {
            shader_stage: naga::ShaderStage::Compute,
            entry_point: "main".into(),
        }),
    )
    .unwrap();

    let out_dir = std::env::var("OUT_DIR").unwrap();
    let out_path = std::path::Path::new(&out_dir).join("transcoding_shader.spv");
    let bytes: Vec<u8> = compiled
        .iter()
        .flat_map(|word| word.to_le_bytes())
        .collect();
    std::fs::write(&out_path, bytes).unwrap();
}

#[cfg(all(feature = "quicksync", target_os = "linux"))]
fn build_quicksync_bindings() {
    println!("cargo:rerun-if-changed=build.rs");

    build_quicksync_vpl();
    build_quicksync_va();
}

#[cfg(all(feature = "quicksync", target_os = "linux"))]
fn build_quicksync_vpl() {
    use std::path::PathBuf;

    let out_dir = PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    let libvpl = pkg_config::Config::new()
        .atleast_version("2.10")
        .probe("vpl")
        .expect("failed to find libvpl; install libvpl development headers");
    let include_dir = libvpl
        .include_paths
        .iter()
        .find(|path| path.join("vpl/mfx.h").exists())
        .expect("libvpl pkg-config metadata did not include the directory containing vpl/mfx.h");
    let mfx_header = include_dir.join("vpl/mfx.h");

    // Adopted from shiguredo/vpl-rs: this is the only build slice we need for
    // current H.264 and future codec wrappers over the same official libvpl API.
    let mut builder = bindgen::Builder::default()
        .header(mfx_header.display().to_string())
        .clang_arg("-DONEVPL_EXPERIMENTAL")
        .allowlist_function("MFX.*")
        .allowlist_type("_?mfx.*")
        .allowlist_type("mfx.*")
        .allowlist_var("MFX.*")
        .allowlist_var("mfx.*")
        .generate_comments(false)
        .derive_debug(false)
        .derive_default(false);
    for include_dir in libvpl.include_paths {
        builder = builder.clang_arg(format!("-I{}", include_dir.display()));
    }
    let bindings = builder
        .generate()
        .expect("failed to generate libvpl bindings");
    std::fs::write(out_dir.join("vpl_bindings.rs"), bindings.to_string())
        .expect("failed to write libvpl bindings");
}

#[cfg(all(feature = "quicksync", target_os = "linux"))]
fn build_quicksync_va() {
    use std::path::PathBuf;

    let out_dir = PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    let libva = pkg_config::Config::new()
        .probe("libva")
        .expect("failed to find libva; install libva development headers");
    let libva_drm = pkg_config::Config::new()
        .probe("libva-drm")
        .expect("failed to find libva-drm; install libva development headers");

    let mut builder = bindgen::Builder::default()
        .header_contents(
            "va_wrapper.h",
            "#include <va/va.h>\n#include <va/va_drm.h>\n#include <va/va_drmcommon.h>\n",
        )
        .allowlist_function(
            "va(GetDisplayDRM|Initialize|Terminate|ExportSurfaceHandle|CreateSurfaces|DestroySurfaces)",
        )
        .allowlist_type(
            "(VADisplay|VASurfaceID|VAStatus|VASurfaceAttrib.*|VAGenericValue.*|.*VADRMPRIME.*)",
        )
        .allowlist_var(
            "VA_(STATUS_SUCCESS|EXPORT_SURFACE_READ_WRITE|EXPORT_SURFACE_SEPARATE_LAYERS|SURFACE_ATTRIB_MEM_TYPE_DRM_PRIME_2|SURFACE_ATTRIB_SETTABLE|RT_FORMAT_YUV420|FOURCC_NV12)",
        )
        .generate_comments(false)
        .derive_debug(false)
        .derive_default(false);
    for include_dir in libva
        .include_paths
        .into_iter()
        .chain(libva_drm.include_paths)
    {
        builder = builder.clang_arg(format!("-I{}", include_dir.display()));
    }
    let bindings = builder
        .generate()
        .expect("failed to generate libva bindings");
    std::fs::write(out_dir.join("va_bindings.rs"), bindings.to_string())
        .expect("failed to write libva bindings");
}
