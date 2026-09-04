fn main() {
    #[cfg(feature = "transcoder")]
    build_transcoding_shader();

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
        video_toolbox: { target_vendor = "apple" },
        supported: { any(vulkan, video_toolbox) }
    }
}

// `#[cfg]` in a build script reflects the host, not the target, so gate on the
// target via env vars instead.
#[cfg(feature = "transcoder")]
fn build_transcoding_shader() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap();
    let is_unix = std::env::var("CARGO_CFG_TARGET_FAMILY")
        .unwrap_or_default()
        .split(',')
        .any(|family| family == "unix");
    let is_vulkan_target = target_os == "windows"
        || (is_unix && !matches!(target_os.as_str(), "macos" | "ios" | "emscripten"));
    if !is_vulkan_target {
        return;
    }

    println!("cargo:rerun-if-changed=src/backends/vulkan/vulkan_transcoder/shader.wgsl");

    let mut front = naga::front::wgsl::Frontend::new();
    let parsed = front
        .parse(include_str!(
            "src/backends/vulkan/vulkan_transcoder/shader.wgsl"
        ))
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
