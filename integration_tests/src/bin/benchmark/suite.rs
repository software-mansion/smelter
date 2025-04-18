use compositor_pipeline::pipeline::{
    encoder::ffmpeg_h264::EncoderPreset, GraphicsContext, VideoDecoder,
};

use crate::{
    args::ResolutionPreset,
    benchmark::{Benchmark, EncoderOptions, ValueOrMaximized},
    benchmark_pass::InputFile,
    scenes::{
        blank, four_video_layout, simple_tiles_with_all_inputs, single_video_layout,
        single_video_pass_through, static_image, two_video_layout, SceneBuilderFn,
    },
    utils::{
        ensure_bunny_1080p30fps, ensure_bunny_1080p60fps, ensure_bunny_2160p30fps,
        ensure_bunny_720p24fps, generate_yuv_from_mp4,
    },
};

pub fn full_benchmark_suite(ctx: &GraphicsContext) -> Vec<Benchmark> {
    let bbb_mp4_720p24fps = ensure_bunny_720p24fps().unwrap();
    let bbb_mp4_1080p30fps = ensure_bunny_1080p30fps().unwrap();
    let bbb_mp4_1080p60fps = ensure_bunny_1080p60fps().unwrap();
    let bbb_mp4_2160p30fps = ensure_bunny_2160p30fps().unwrap();
    let bbb_raw_720p_input = InputFile::Raw(generate_yuv_from_mp4(&bbb_mp4_720p24fps).unwrap());
    let supports_vk_video = ctx.vulkan_ctx.is_some();

    let const_input_output_ratio_scenes: [(&'static str, SceneBuilderFn, u64); 3] = [
        ("1 input per output", single_video_layout, 1),
        ("2 inputs per output", two_video_layout, 2),
        ("4 inputs per output", four_video_layout, 4),
    ];
    let const_input_output_ratio_encoder_presets = [EncoderPreset::Ultrafast, EncoderPreset::Fast];

    let const_input_output_ratio_input = [
        (
            "bbb_mp4_720p24fps",
            InputFile::Mp4(bbb_mp4_720p24fps.clone()),
        ),
        (
            "bbb_mp4_1080p30fps",
            InputFile::Mp4(bbb_mp4_1080p30fps.clone()),
        ),
        (
            "bbb_mp4_2160p30fps",
            InputFile::Mp4(bbb_mp4_2160p30fps.clone()),
        ),
    ];

    let const_input_output_ratio_decoder = match supports_vk_video {
        true => vec![VideoDecoder::VulkanVideoH264, VideoDecoder::FFmpegH264],
        false => vec![VideoDecoder::FFmpegH264],
    };

    let const_input_output_ratio = const_input_output_ratio_decoder
        .iter()
        .flat_map(|decoder| {
            const_input_output_ratio_scenes
                .iter()
                .flat_map(|scene| {
                    const_input_output_ratio_encoder_presets
                        .iter()
                        .flat_map(|preset| {
                            const_input_output_ratio_input
                                .iter()
                                .map(|input| (scene, preset, input, decoder))
                                .collect::<Vec<_>>()
                        })
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>()
        })
        .map(|(scene, encoder, input, decoder)| Benchmark {
            id: format!(
                "{} - preset: {:?}, input: {}, decoder: {:?}",
                scene.0, encoder, input.0, decoder
            )
            .leak(),
            input_count: ValueOrMaximized::MaximizeWithInitial(scene.2),
            output_count: ValueOrMaximized::MaximizeWithInitial(1),
            output_resolution: ValueOrMaximized::Value(ResolutionPreset::Res1080p.into()),
            framerate: ValueOrMaximized::Value(30),
            scene_builder: scene.1,
            encoder: EncoderOptions::Enabled(*encoder),
            decoder: *decoder,
            input_file: input.1.clone(),
            ..Default::default()
        })
        .collect::<Vec<_>>();

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
        input_file: bbb_raw_720p_input.clone(),
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
        input_file: bbb_raw_720p_input.clone(), // no decoder
        ..Default::default()
    })
    .collect::<Vec<_>>();

    let renderer_only_benchmarks = [
        (blank as SceneBuilderFn, "blank"),
        (simple_tiles_with_all_inputs, "simple_tiles_with_all_inputs"),
        (single_video_layout, "single_video_layout"),
        (single_video_pass_through, "single_video_pass_through"),
        (static_image, "static_image"),
    ]
    .into_iter()
    .map(|(func, func_name)| Benchmark {
        id: format!("rendering only - scene: {:?}", func_name).leak(),
        input_count: ValueOrMaximized::Value(1),
        output_count: ValueOrMaximized::Maximize,
        framerate: ValueOrMaximized::Value(30),
        encoder: EncoderOptions::Disabled,
        scene_builder: func,
        input_file: bbb_raw_720p_input.clone(), // no decoder
        ..Default::default()
    })
    .collect::<Vec<_>>();

    let decoder_only_inputs = [
        (
            "bbb_mp4_720p24fps",
            InputFile::Mp4(bbb_mp4_720p24fps.clone()),
        ),
        (
            "bbb_mp4_1080p30fps",
            InputFile::Mp4(bbb_mp4_1080p30fps.clone()),
        ),
        (
            "bbb_mp4_1080p60fps",
            InputFile::Mp4(bbb_mp4_1080p60fps.clone()),
        ),
        (
            "bbb_mp4_2160p30fps",
            InputFile::Mp4(bbb_mp4_2160p30fps.clone()),
        ),
    ];
    let decoder_only = match supports_vk_video {
        true => vec![VideoDecoder::FFmpegH264, VideoDecoder::VulkanVideoH264],
        false => vec![VideoDecoder::FFmpegH264],
    }
    .iter()
    .flat_map(|decoder| {
        decoder_only_inputs
            .iter()
            .map(|input| (input, decoder))
            .collect::<Vec<_>>()
    })
    .map(|(input, decoder)| Benchmark {
        id: format!("decoding only - decoder: {:?}, input: {}", decoder, input.0).leak(),
        input_count: ValueOrMaximized::Maximize,
        output_count: ValueOrMaximized::Value(1),
        output_resolution: ValueOrMaximized::Value(ResolutionPreset::Res144p.into()),
        framerate: ValueOrMaximized::Value(30),
        encoder: EncoderOptions::Disabled,
        decoder: *decoder,
        scene_builder: blank,
        input_file: input.1.clone(),
        ..Default::default()
    })
    .collect::<Vec<_>>();

    [
        // benchmarks that increases outputs and input in the same ratio with all comibnations of
        // - different input resolutions/fps
        // - different encoder presets
        // - different input/output ratios
        const_input_output_ratio,
        // benchmark multiple encoding presets with ffmpeg encoder / single input no decoder
        ffmpeg_h264_encoder_preset_benchmarks,
        // benchmark multiple output resolutions with encoder / single input no decoder
        output_resolution_benchmarks,
        // rendering only / multiple outputs no encoder / single input no decoder
        renderer_only_benchmarks,
        // decoder only tests / one output low resolution no encoder blank scene
        decoder_only,
    ]
    .concat()
}

pub fn minimal_benchmark_suite() -> Vec<Benchmark> {
    let bbb_mp4_720p24fps = ensure_bunny_720p24fps().unwrap();
    let bbb_raw_720p_input = InputFile::Raw(generate_yuv_from_mp4(&bbb_mp4_720p24fps).unwrap());
    let bbb_mp4_720p_input = InputFile::Mp4(bbb_mp4_720p24fps.clone());

    [
        Benchmark {
            id: "simple (+decoder, +encoder) max outputs when (2 inputs per output)",
            input_count: ValueOrMaximized::MaximizeWithInitial(2),
            output_count: ValueOrMaximized::MaximizeWithInitial(1),
            framerate: ValueOrMaximized::Value(30),
            output_resolution: ValueOrMaximized::Value(ResolutionPreset::Res1080p.into()),
            scene_builder: two_video_layout,
            input_file: bbb_mp4_720p_input.clone(),
            ..Default::default()
        },
        Benchmark {
            id: "simple (+decoder, +encoder) max inputs",
            input_count: ValueOrMaximized::Maximize,
            output_count: ValueOrMaximized::Value(1),
            framerate: ValueOrMaximized::Value(30),
            output_resolution: ValueOrMaximized::Value(ResolutionPreset::Res1080p.into()),
            scene_builder: simple_tiles_with_all_inputs,
            input_file: bbb_mp4_720p_input.clone(),
            ..Default::default()
        },
        Benchmark {
            id: "simple (-decoder, +encoder) max inputs",
            input_count: ValueOrMaximized::Maximize,
            output_count: ValueOrMaximized::Value(1),
            framerate: ValueOrMaximized::Value(30),
            output_resolution: ValueOrMaximized::Value(ResolutionPreset::Res1080p.into()),
            scene_builder: simple_tiles_with_all_inputs,
            input_file: bbb_raw_720p_input.clone(), // disable decoder
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
            input_file: bbb_mp4_720p_input.clone(),
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
            input_file: bbb_raw_720p_input.clone(), // disable decoder
            ..Default::default()
        },
    ]
    .into()
}
