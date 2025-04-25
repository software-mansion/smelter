use compositor_pipeline::pipeline::encoder::ffmpeg_h264::EncoderPreset;

use crate::{
    args::ResolutionPreset,
    benchmark::{Benchmark, EncoderOptions, ValueOrMaximized},
    benchmark_pass::InputFile,
    scenes::simple_tiles_with_all_inputs,
    utils::{ensure_default_mp4, generate_yuv_from_mp4},
};

pub fn full_benchmark_suite() -> Vec<Benchmark> {
    // BigBuckBunny 1280x720
    let mp4_path = ensure_default_mp4().unwrap();
    let raw_input = InputFile::Raw(generate_yuv_from_mp4(&mp4_path).unwrap());
    // let mp4_input = InputFile::Mp4(mp4_path);

    let ffmpeg_h264_encoder_preset_benchmarks = [
        EncoderPreset::Ultrafast,
        EncoderPreset::Superfast,
        EncoderPreset::Veryfast,
        EncoderPreset::Faster,
        EncoderPreset::Fast,
        EncoderPreset::Medium,
    ]
    .into_iter()
    .map(|encoder_preset| Benchmark {
        id: format!("ffmpeg_h264 - encoder_preset: {:?}", encoder_preset).leak(),
        input_count: ValueOrMaximized::Value(1),
        output_count: ValueOrMaximized::Maximize,
        framerate: ValueOrMaximized::Value(30),
        output_resolution: ValueOrMaximized::Value(ResolutionPreset::Res1080p.into()),
        encoder: EncoderOptions::Enabled(encoder_preset),
        scene_builder: simple_tiles_with_all_inputs,
        input_file: raw_input.clone(),
        ..Default::default()
    })
    .collect::<Vec<_>>();

    let output_resolution_benchmarks = [
        ResolutionPreset::Res2160p,
        ResolutionPreset::Res1440p,
        ResolutionPreset::Res1080p,
        ResolutionPreset::Res720p,
        ResolutionPreset::Res480p,
    ]
    .into_iter()
    .map(|resolution_preset| Benchmark {
        id: format!("resolution: {:?}", resolution_preset).leak(),
        input_count: ValueOrMaximized::Value(1),
        output_count: ValueOrMaximized::Maximize,
        framerate: ValueOrMaximized::Value(30),
        output_resolution: ValueOrMaximized::Value(resolution_preset.into()),
        scene_builder: simple_tiles_with_all_inputs,
        input_file: raw_input.clone(), // no decoder
        ..Default::default()
    })
    .collect::<Vec<_>>();

    let renderer_only_benchmarks = [
        (simple_tiles_with_all_inputs, "simple_tiles_with_all_inputs"),
        // TODO: add more scenes
        // - each output with one input
        // - don't use inputs in renders
        // - render static image
    ]
    .into_iter()
    .map(|(func, func_name)| Benchmark {
        id: format!("rendering only - scene: {:?}", func_name).leak(),
        input_count: ValueOrMaximized::Value(1),
        output_count: ValueOrMaximized::Maximize,
        framerate: ValueOrMaximized::Value(30),
        encoder: EncoderOptions::Disabled,
        scene_builder: func,
        input_file: raw_input.clone(), // no decoder
        ..Default::default()
    })
    .collect::<Vec<_>>();

    [
        // benchmark multiple encoding presets with ffmpeg encoder / single input no decoder
        ffmpeg_h264_encoder_preset_benchmarks,
        // benchmark multiple output resolutions with encoder / single input no decoder
        output_resolution_benchmarks,
        // rendering only / multiple outputs no encoder / single input no decoder
        renderer_only_benchmarks,
        // TODO: decoder only tests / one output low resolution no encoder
    ]
    .concat()
}

pub fn minimal_benchmark_suite() -> Vec<Benchmark> {
    // BigBuckBunny 1280x720
    let mp4_path = ensure_default_mp4().unwrap();
    let raw_input = InputFile::Raw(generate_yuv_from_mp4(&mp4_path).unwrap());
    let mp4_input = InputFile::Mp4(mp4_path);

    [
        Benchmark {
            id: "simple (+decoder, +encoder) max inputs",
            input_count: ValueOrMaximized::Maximize,
            output_count: ValueOrMaximized::Value(1),
            framerate: ValueOrMaximized::Value(30),
            output_resolution: ValueOrMaximized::Value(ResolutionPreset::Res1080p.into()),
            scene_builder: simple_tiles_with_all_inputs,
            input_file: mp4_input.clone(),
            ..Default::default()
        },
        Benchmark {
            id: "simple (-decoder, +encoder) max inputs",
            input_count: ValueOrMaximized::Maximize,
            output_count: ValueOrMaximized::Value(1),
            framerate: ValueOrMaximized::Value(30),
            output_resolution: ValueOrMaximized::Value(ResolutionPreset::Res1080p.into()),
            scene_builder: simple_tiles_with_all_inputs,
            input_file: raw_input.clone(), // disable decoder
            ..Default::default()
        },
        Benchmark {
            id: "simple (+decoder, -encoder) max inputs",
            input_count: ValueOrMaximized::Maximize,
            output_count: ValueOrMaximized::Value(1),
            framerate: ValueOrMaximized::Value(30),
            output_resolution: ValueOrMaximized::Value(ResolutionPreset::Res1080p.into()),
            scene_builder: simple_tiles_with_all_inputs,
            encoder: EncoderOptions::Disabled,
            input_file: mp4_input.clone(),
            ..Default::default()
        },
        Benchmark {
            id: "simple (-decoder, -encoder) max inputs",
            input_count: ValueOrMaximized::Maximize,
            output_count: ValueOrMaximized::Value(1),
            framerate: ValueOrMaximized::Value(30),
            output_resolution: ValueOrMaximized::Value(ResolutionPreset::Res1080p.into()),
            scene_builder: simple_tiles_with_all_inputs,
            encoder: EncoderOptions::Disabled,
            input_file: raw_input.clone(), // disable decoder
            ..Default::default()
        },
    ]
    .into()
}
