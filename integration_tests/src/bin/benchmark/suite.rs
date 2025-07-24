use std::{path::PathBuf, sync::Arc, time::Duration};

use compositor_pipeline::{
    codecs::{FfmpegH264EncoderPreset, VideoDecoderOptions},
    graphics_context::GraphicsContext,
};
use compositor_render::RenderingMode;

use crate::{
    args::{Resolution, ResolutionPreset},
    benchmark::{Benchmark, EncoderOptions},
    benchmark_pass::{InputFile, SingleBenchmarkPass},
    scenes::{
        blank, example_image, example_shader, four_video_layout, image_with_shader,
        simple_tiles_with_all_inputs, single_video_layout, single_video_pass_through, static_image,
        two_video_layout, SceneBuilderFn,
    },
    utils::{
        ensure_bunny_1080p30fps, ensure_bunny_1080p60fps, ensure_bunny_2160p30fps,
        ensure_bunny_480p24fps, ensure_bunny_720p24fps, generate_png_from_video,
        generate_yuv_from_mp4,
    },
};

struct BenchmarkSuiteContext {
    #[allow(dead_code)]
    wgpu_ctx: GraphicsContext,
    bbb_mp4_480p24fps: PathBuf,
    bbb_mp4_720p24fps: PathBuf,
    bbb_mp4_1080p30fps: PathBuf,
    bbb_mp4_1080p60fps: PathBuf,
    bbb_mp4_2160p30fps: PathBuf,
    bbb_raw_720p_input: InputFile,

    default_resolution: Resolution,
    default_framerate: u64,
    default_rendering_mode: RenderingMode,
}

impl BenchmarkSuiteContext {
    fn new(
        wgpu_ctx: &GraphicsContext,
        default_resolution: Resolution,
        default_framerate: u64,
        default_rendering_mode: RenderingMode,
    ) -> &'static Self {
        let bbb_mp4_480p24fps = ensure_bunny_480p24fps().unwrap();
        let bbb_mp4_720p24fps = ensure_bunny_720p24fps().unwrap();
        let bbb_mp4_1080p30fps = ensure_bunny_1080p30fps().unwrap();
        let bbb_mp4_1080p60fps = ensure_bunny_1080p60fps().unwrap();
        let bbb_mp4_2160p30fps = ensure_bunny_2160p30fps().unwrap();

        generate_png_from_video(&bbb_mp4_1080p30fps);
        Box::leak(
            Self {
                wgpu_ctx: wgpu_ctx.clone(),
                bbb_raw_720p_input: InputFile::Raw(
                    generate_yuv_from_mp4(&bbb_mp4_720p24fps).unwrap(),
                ),
                bbb_mp4_480p24fps,
                bbb_mp4_720p24fps,
                bbb_mp4_1080p30fps,
                bbb_mp4_1080p60fps,
                bbb_mp4_2160p30fps,

                default_resolution,
                default_framerate,
                default_rendering_mode,
            }
            .into(),
        )
    }

    fn default(&self) -> SingleBenchmarkPass {
        SingleBenchmarkPass {
            scene_builder: simple_tiles_with_all_inputs,
            resources: vec![],

            input_count: 1,
            output_count: 1,
            framerate: self.default_framerate,
            output_resolution: self.default_resolution,

            input_file: InputFile::Mp4(PathBuf::new()), // always override

            encoder: EncoderOptions::Enabled(FfmpegH264EncoderPreset::Ultrafast),
            decoder: VideoDecoderOptions::FfmpegH264,

            warm_up_time: Duration::from_secs(2),
            rendering_mode: self.default_rendering_mode,
        }
    }
}

pub fn full_benchmark_suite(ctx: &GraphicsContext) -> Vec<Benchmark> {
    let ctx = BenchmarkSuiteContext::new(
        ctx,
        Resolution {
            width: 1920,
            height: 1080,
        },
        30,
        match ctx.adapter.get_info().device_type {
            wgpu::DeviceType::Cpu => RenderingMode::CpuOptimized,
            _ => RenderingMode::GpuOptimized,
        },
    );

    [
        // benchmarks that increases outputs and input in the same ratio with all combinations of
        // - different input resolutions/fps
        // - different encoder presets
        // - different input/output ratios
        benchmark_set_constant_input_output_ratio(ctx),
        // benchmark multiple encoding presets with ffmpeg encoder / single input no decoder
        benchmark_set_ffmpeg_h264_encoder_preset(ctx),
        // benchmark multiple output resolutions with encoder / single input no decoder
        benchmark_set_output_resolutions(ctx),
        // rendering only / multiple outputs no encoder / single input no decoder
        benchmark_set_renderer_only(ctx),
        // decoder only tests / one output low resolution no encoder blank scene
        benchmark_set_decoder_only(ctx),
    ]
    .concat()
}

pub fn cpu_optimized_benchmark_suite(ctx: &GraphicsContext) -> Vec<Benchmark> {
    let ctx_720p = BenchmarkSuiteContext::new(
        ctx,
        Resolution {
            width: 1270,
            height: 720,
        },
        24,
        RenderingMode::CpuOptimized,
    );
    let ctx_480p = BenchmarkSuiteContext::new(
        ctx,
        Resolution {
            width: 854,
            height: 480,
        },
        24,
        RenderingMode::CpuOptimized,
    );

    [
        // benchmarks that increases outputs and input in the same ratio with all combinations of
        // - different input resolutions/fps
        // - different encoder presets
        // - different input/output ratios
        benchmark_set_constant_input_output_ratio(ctx_480p),
        benchmark_set_constant_input_output_ratio(ctx_720p),
        // rendering only / multiple outputs no encoder / single input no decoder
        benchmark_set_renderer_only(ctx_480p),
        benchmark_set_renderer_only(ctx_720p),
    ]
    .concat()
}

pub fn minimal_benchmark_suite(ctx: &GraphicsContext) -> Vec<Benchmark> {
    let ctx = BenchmarkSuiteContext::new(
        ctx,
        Resolution {
            width: 1270,
            height: 720,
        },
        24,
        RenderingMode::GpuOptimized,
    );

    [
        Benchmark {
            id: "simple (+decoder, +encoder) max outputs when (2 inputs per output)",
            bench_pass_builder: Arc::new(Box::new(|value: u64| SingleBenchmarkPass {
                input_count: value * 2,
                output_count: value,
                output_resolution: ResolutionPreset::Res1080p.into(),
                scene_builder: two_video_layout,
                input_file: InputFile::Mp4(ctx.bbb_mp4_720p24fps.clone()),
                ..ctx.default()
            })),
        },
        Benchmark {
            id: "simple (+decoder, +encoder) max inputs",
            bench_pass_builder: Arc::new(Box::new(|value: u64| SingleBenchmarkPass {
                input_count: value,
                output_count: 1,
                output_resolution: ResolutionPreset::Res1080p.into(),
                scene_builder: two_video_layout,
                input_file: InputFile::Mp4(ctx.bbb_mp4_720p24fps.clone()),
                ..ctx.default()
            })),
        },
        Benchmark {
            id: "simple (-decoder, +encoder) max inputs",
            bench_pass_builder: Arc::new(Box::new(|value: u64| SingleBenchmarkPass {
                input_count: value,
                output_count: 1,
                output_resolution: ResolutionPreset::Res1080p.into(),
                scene_builder: two_video_layout,
                input_file: ctx.bbb_raw_720p_input.clone(), // disable decoder
                ..ctx.default()
            })),
        },
        Benchmark {
            id: "simple (+decoder, -encoder) max inputs",
            bench_pass_builder: Arc::new(Box::new(|value: u64| SingleBenchmarkPass {
                input_count: value,
                output_count: 1,
                output_resolution: ResolutionPreset::Res1080p.into(),
                scene_builder: two_video_layout,
                encoder: EncoderOptions::Disabled,
                input_file: InputFile::Mp4(ctx.bbb_mp4_720p24fps.clone()),
                ..ctx.default()
            })),
        },
        Benchmark {
            id: "simple (-decoder, -encoder) max inputs",
            bench_pass_builder: Arc::new(Box::new(|value: u64| SingleBenchmarkPass {
                input_count: value,
                output_count: 1,
                output_resolution: ResolutionPreset::Res1080p.into(),
                scene_builder: two_video_layout,
                encoder: EncoderOptions::Disabled,
                input_file: ctx.bbb_raw_720p_input.clone(), // disable decoder
                ..ctx.default()
            })),
        },
    ]
    .into()
}

fn benchmark_set_constant_input_output_ratio(
    ctx: &'static BenchmarkSuiteContext,
) -> Vec<Benchmark> {
    let const_input_output_ratio_scenes: [(&'static str, SceneBuilderFn, u64); 3] = [
        ("1 input per output", single_video_layout, 1),
        ("2 inputs per output", two_video_layout, 2),
        ("4 inputs per output", four_video_layout, 4),
    ];
    let const_input_output_ratio_encoder_presets = [
        FfmpegH264EncoderPreset::Ultrafast,
        FfmpegH264EncoderPreset::Veryfast,
        FfmpegH264EncoderPreset::Fast,
    ];

    let const_input_output_ratio_input = [
        (
            "bbb_mp4_480p24fps",
            InputFile::Mp4(ctx.bbb_mp4_480p24fps.clone()),
        ),
        (
            "bbb_mp4_720p24fps",
            InputFile::Mp4(ctx.bbb_mp4_720p24fps.clone()),
        ),
        (
            "bbb_mp4_1080p30fps",
            InputFile::Mp4(ctx.bbb_mp4_1080p30fps.clone()),
        ),
        (
            "bbb_mp4_2160p30fps",
            InputFile::Mp4(ctx.bbb_mp4_2160p30fps.clone()),
        ),
    ];

    let const_input_output_ratio_decoder = supported_decoders(ctx);

    const_input_output_ratio_decoder
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
                                .map(|input| (*scene, *preset, input.clone(), *decoder))
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
            bench_pass_builder: Arc::new(Box::new(move |value: u64| SingleBenchmarkPass {
                input_count: value * scene.2,
                output_count: value,
                scene_builder: scene.1,
                encoder: EncoderOptions::Enabled(encoder),
                decoder,
                input_file: input.1.clone(),
                ..ctx.default()
            })),
        })
        .collect()
}

fn benchmark_set_ffmpeg_h264_encoder_preset(ctx: &'static BenchmarkSuiteContext) -> Vec<Benchmark> {
    [
        FfmpegH264EncoderPreset::Fast,
        FfmpegH264EncoderPreset::Ultrafast,
        FfmpegH264EncoderPreset::Superfast,
        FfmpegH264EncoderPreset::Veryfast,
        FfmpegH264EncoderPreset::Faster,
        FfmpegH264EncoderPreset::Fast,
        FfmpegH264EncoderPreset::Medium,
    ]
    .into_iter()
    .map(|encoder_preset| Benchmark {
        id: format!("ffmpeg_h264 - encoder_preset: {encoder_preset:?}").leak(),
        bench_pass_builder: Arc::new(Box::new(move |value: u64| SingleBenchmarkPass {
            input_count: 1,
            output_count: value,
            scene_builder: simple_tiles_with_all_inputs,
            encoder: EncoderOptions::Enabled(encoder_preset),
            input_file: ctx.bbb_raw_720p_input.clone(), // no decoder
            ..ctx.default()
        })),
    })
    .collect()
}

fn benchmark_set_output_resolutions(ctx: &'static BenchmarkSuiteContext) -> Vec<Benchmark> {
    [
        ResolutionPreset::Res2160p,
        ResolutionPreset::Res1440p,
        ResolutionPreset::Res1080p,
        ResolutionPreset::Res720p,
        ResolutionPreset::Res480p,
    ]
    .into_iter()
    .map(|resolution_preset| Benchmark {
        id: format!("resolution: {resolution_preset:?}").leak(),
        bench_pass_builder: Arc::new(Box::new(move |value: u64| SingleBenchmarkPass {
            input_count: 1,
            output_count: value,
            output_resolution: resolution_preset.into(),
            scene_builder: simple_tiles_with_all_inputs,
            encoder: EncoderOptions::Enabled(FfmpegH264EncoderPreset::Veryfast),
            input_file: ctx.bbb_raw_720p_input.clone(), // no decoder
            ..ctx.default()
        })),
    })
    .collect()
}

fn benchmark_set_renderer_only(ctx: &'static BenchmarkSuiteContext) -> Vec<Benchmark> {
    [
        (blank as SceneBuilderFn, "blank", vec![]),
        (
            simple_tiles_with_all_inputs,
            "simple_tiles_with_all_inputs",
            vec![],
        ),
        (single_video_layout, "single_video_layout", vec![]),
        // TODO: breaks xwayland (registers 1000 outputs)
        (
            single_video_pass_through,
            "single_video_pass_through",
            vec![],
        ),
        (static_image, "static_image", vec![example_image()]),
        (
            image_with_shader,
            "image_with_shader",
            vec![example_image(), example_shader()],
        ),
    ]
    .into_iter()
    .map(|(func, func_name, resources)| Benchmark {
        id: format!("rendering only - scene: {func_name:?}").leak(),
        bench_pass_builder: Arc::new(Box::new(move |value: u64| SingleBenchmarkPass {
            input_count: 1,
            output_count: value,
            encoder: EncoderOptions::Disabled,
            scene_builder: func,
            resources: resources.clone(),
            input_file: ctx.bbb_raw_720p_input.clone(),
            ..ctx.default()
        })),
    })
    .collect()
}

fn benchmark_set_decoder_only(ctx: &'static BenchmarkSuiteContext) -> Vec<Benchmark> {
    let decoder_only_inputs = [
        (
            "bbb_mp4_480p24fps",
            InputFile::Mp4(ctx.bbb_mp4_480p24fps.clone()),
        ),
        (
            "bbb_mp4_720p24fps",
            InputFile::Mp4(ctx.bbb_mp4_720p24fps.clone()),
        ),
        (
            "bbb_mp4_1080p30fps",
            InputFile::Mp4(ctx.bbb_mp4_1080p30fps.clone()),
        ),
        (
            "bbb_mp4_1080p60fps",
            InputFile::Mp4(ctx.bbb_mp4_1080p60fps.clone()),
        ),
        (
            "bbb_mp4_2160p30fps",
            InputFile::Mp4(ctx.bbb_mp4_2160p30fps.clone()),
        ),
    ];

    supported_decoders(ctx)
        .iter()
        .flat_map(|decoder| {
            decoder_only_inputs
                .iter()
                .map(|input| (input.clone(), *decoder))
                .collect::<Vec<_>>()
        })
        .map(|(input, decoder)| Benchmark {
            id: format!("decoding only - decoder: {:?}, input: {}", decoder, input.0).leak(),
            bench_pass_builder: Arc::new(Box::new(move |value: u64| SingleBenchmarkPass {
                input_count: value,
                output_count: 1,
                output_resolution: ResolutionPreset::Res144p.into(),
                encoder: EncoderOptions::Disabled,
                decoder,
                input_file: input.1.clone(),
                scene_builder: blank,
                ..ctx.default()
            })),
        })
        .collect()
}

#[cfg(not(target_os = "macos"))]
fn supported_decoders(ctx: &BenchmarkSuiteContext) -> Vec<VideoDecoderOptions> {
    match ctx.wgpu_ctx.vulkan_ctx.is_some() {
        true => vec![
            VideoDecoderOptions::VulkanH264,
            VideoDecoderOptions::FfmpegH264,
        ],
        false => vec![VideoDecoderOptions::FfmpegH264],
    }
}

#[cfg(target_os = "macos")]
fn supported_decoders(_ctx: &BenchmarkSuiteContext) -> Vec<VideoDecoderOptions> {
    vec![VideoDecoderOptions::FfmpegH264]
}
