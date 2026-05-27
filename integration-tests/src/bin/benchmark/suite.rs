use std::{path::PathBuf, sync::Arc, time::Duration};

use smelter_core::{
    codecs::{FfmpegH264EncoderPreset, VideoDecoderOptions},
    graphics_context::GraphicsContext,
};
use smelter_render::RenderingMode;

use crate::{
    args::ResolutionPreset,
    benchmark::{Benchmark, EncoderOptions},
    benchmark_pass::{InputFile, InputFileKind, SingleBenchmarkPass},
    scenes::{
        BLANK_1_TO_N, BLANK_N_TO_1, BuilderFn, FOUR_VIDEO_4N_TO_N, IMAGE_WITH_SHADER_1_TO_N,
        PASS_THROUGH_1_TO_N, SINGLE_VIDEO_1_TO_N, SINGLE_VIDEO_N_TO_N, STATIC_IMAGE_1_TO_N,
        SceneLayout, TILES_1_TO_N, TWO_VIDEO_2N_TO_N,
    },
    utils::{
        ensure_bunny_480p24fps, ensure_bunny_720p24fps, ensure_bunny_1080p30fps,
        ensure_bunny_1080p60fps_no_audio, ensure_bunny_2160p30fps, generate_png_from_video,
        generate_yuv_from_mp4,
    },
};

pub fn full_benchmark_suite(ctx: &GraphicsContext) -> Vec<Benchmark> {
    let ctx = BenchmarkSuiteContext::new(ctx);

    [
        // benchmarks that increases outputs and input in the same ratio with all combinations of
        benchmark_set_constant_input_output_ratio()
            .output_resolutions([ResolutionPreset::Res2160p])
            .input_source([
                ctx.bbb_mp4_1080p30fps.clone(),
                ctx.bbb_mp4_2160p30fps.clone(),
            ])
            .build(),
        benchmark_set_constant_input_output_ratio()
            .output_resolutions([ResolutionPreset::Res1440p])
            .input_source([
                ctx.bbb_mp4_720p24fps.clone(),
                ctx.bbb_mp4_1080p30fps.clone(),
                ctx.bbb_mp4_2160p30fps.clone(),
            ])
            .build(),
        benchmark_set_constant_input_output_ratio()
            .output_resolutions([ResolutionPreset::Res1080p])
            .input_source([
                ctx.bbb_mp4_720p24fps.clone(),
                ctx.bbb_mp4_1080p30fps.clone(),
                ctx.bbb_mp4_2160p30fps.clone(),
            ])
            .build(),
        benchmark_set_constant_input_output_ratio()
            .output_resolutions([ResolutionPreset::Res720p])
            .input_source([
                ctx.bbb_mp4_480p24fps.clone(),
                ctx.bbb_mp4_720p24fps.clone(),
                ctx.bbb_mp4_1080p30fps.clone(),
            ])
            .build(),
        // rendering only / multiple outputs no encoder / single input no decoder
        benchmark_set_renderer_only(ctx).build(),
        // decoder only tests / one output low resolution no encoder blank scene
        benchmark_set_decoder_only(ctx).build(),
        // encoder only tests / one input passthrough scene
        benchmark_set_encoder_only(ctx).build(),
    ]
    .concat()
}

/// encoder only tests / one input passthrough scene
pub fn encoder_only_benchmark_suite(ctx: &GraphicsContext) -> Vec<Benchmark> {
    let ctx = BenchmarkSuiteContext::new(ctx);
    benchmark_set_encoder_only(ctx).build()
}

pub fn decoder_only_benchmark_suite(ctx: &GraphicsContext) -> Vec<Benchmark> {
    let ctx = BenchmarkSuiteContext::new(ctx);
    benchmark_set_decoder_only(ctx).build()
}

pub fn high_res_benchmark_suite(ctx: &GraphicsContext) -> Vec<Benchmark> {
    let ctx = BenchmarkSuiteContext::new(ctx);
    let vk_base = BenchmarkBuilder::new()
        .scenes([SINGLE_VIDEO_N_TO_N, TWO_VIDEO_2N_TO_N, FOUR_VIDEO_4N_TO_N])
        .encoders([EncoderOptions::VulkanH264])
        .decoders(vec![VideoDecoderOptions::VulkanH264]);
    let ffmpeg_base = BenchmarkBuilder::new()
        .scenes([SINGLE_VIDEO_N_TO_N, TWO_VIDEO_2N_TO_N, FOUR_VIDEO_4N_TO_N])
        .encoders([
            EncoderOptions::FfmpegH264(FfmpegH264EncoderPreset::Ultrafast),
            EncoderOptions::FfmpegH264(FfmpegH264EncoderPreset::Fast),
        ])
        .decoders(vec![VideoDecoderOptions::FfmpegH264]);
    [
        vk_base
            .clone()
            .output_resolutions([ResolutionPreset::Res2160p])
            .input_source([
                ctx.bbb_mp4_1080p30fps.clone(),
                ctx.bbb_mp4_2160p30fps.clone(),
            ])
            .build(),
        vk_base
            .clone()
            .output_resolutions([ResolutionPreset::Res1440p])
            .input_source([ctx.bbb_mp4_1080p30fps.clone()])
            .build(),
        vk_base
            .clone()
            .output_resolutions([ResolutionPreset::Res1080p])
            .input_source([ctx.bbb_mp4_1080p30fps.clone()])
            .build(),
        ffmpeg_base
            .clone()
            .output_resolutions([ResolutionPreset::Res2160p])
            .input_source([
                ctx.bbb_mp4_1080p30fps.clone(),
                ctx.bbb_mp4_2160p30fps.clone(),
            ])
            .build(),
        ffmpeg_base
            .clone()
            .output_resolutions([ResolutionPreset::Res1440p])
            .input_source([ctx.bbb_mp4_1080p30fps.clone()])
            .build(),
        ffmpeg_base
            .clone()
            .output_resolutions([ResolutionPreset::Res1080p])
            .input_source([ctx.bbb_mp4_1080p30fps.clone()])
            .build(),
    ]
    .concat()
}

// Basic most realistic scenarios
pub fn minimal_benchmark_suite(ctx: &GraphicsContext) -> Vec<Benchmark> {
    let ctx = BenchmarkSuiteContext::new(ctx);
    let vk_base = BenchmarkBuilder::new()
        .scenes([SINGLE_VIDEO_N_TO_N, FOUR_VIDEO_4N_TO_N])
        .encoders([EncoderOptions::VulkanH264])
        .decoders(vec![VideoDecoderOptions::VulkanH264]);
    let ffmpeg_base = BenchmarkBuilder::new()
        .scenes([SINGLE_VIDEO_N_TO_N, FOUR_VIDEO_4N_TO_N])
        .encoders([
            EncoderOptions::FfmpegH264(FfmpegH264EncoderPreset::Ultrafast),
            EncoderOptions::FfmpegH264(FfmpegH264EncoderPreset::Fast),
        ])
        .decoders(vec![VideoDecoderOptions::FfmpegH264]);

    [
        vk_base
            .clone()
            .input_source([
                ctx.bbb_mp4_1080p30fps.clone(),
                ctx.bbb_mp4_2160p30fps.clone(),
            ])
            .output_resolutions([ResolutionPreset::Res2160p])
            .build(),
        vk_base
            .clone()
            .input_source([
                ctx.bbb_mp4_720p24fps.clone(),
                ctx.bbb_mp4_1080p30fps.clone(),
            ])
            .output_resolutions([ResolutionPreset::Res1080p, ResolutionPreset::Res1440p])
            .build(),
        vk_base
            .clone()
            .input_source([ctx.bbb_mp4_720p24fps.clone()])
            .output_resolutions([ResolutionPreset::Res720p])
            .build(),
        ffmpeg_base
            .clone()
            .input_source([
                ctx.bbb_mp4_1080p30fps.clone(),
                ctx.bbb_mp4_2160p30fps.clone(),
            ])
            .output_resolutions([ResolutionPreset::Res2160p])
            .build(),
        ffmpeg_base
            .clone()
            .input_source([
                ctx.bbb_mp4_720p24fps.clone(),
                ctx.bbb_mp4_1080p30fps.clone(),
            ])
            .output_resolutions([ResolutionPreset::Res1080p, ResolutionPreset::Res1440p])
            .build(),
        ffmpeg_base
            .clone()
            .input_source([ctx.bbb_mp4_720p24fps.clone()])
            .output_resolutions([ResolutionPreset::Res720p])
            .build(),
    ]
    .concat()
}

// benchmark suite need for website docs
pub fn c5_docs_benchmark_suite(ctx: &GraphicsContext) -> Vec<Benchmark> {
    let ctx = BenchmarkSuiteContext::new(ctx);
    let ffmpeg_base = BenchmarkBuilder::new()
        .scenes([SINGLE_VIDEO_N_TO_N, TWO_VIDEO_2N_TO_N, FOUR_VIDEO_4N_TO_N])
        .encoders([
            EncoderOptions::FfmpegH264(FfmpegH264EncoderPreset::Ultrafast),
            EncoderOptions::FfmpegH264(FfmpegH264EncoderPreset::Veryfast),
            EncoderOptions::FfmpegH264(FfmpegH264EncoderPreset::Fast),
        ])
        .decoders(vec![VideoDecoderOptions::FfmpegH264])
        .rendering_mode(RenderingMode::CpuOptimized);

    [
        ffmpeg_base
            .clone()
            .input_source([ctx.bbb_mp4_1080p30fps.clone()])
            .output_resolutions([ResolutionPreset::Res1080p])
            .build(),
        ffmpeg_base
            .clone()
            .input_source([ctx.bbb_mp4_720p24fps.clone()])
            .output_resolutions([ResolutionPreset::Res720p])
            .build(),
    ]
    .concat()
}

// benchmark suite need for website docs
pub fn g4dn_docs_benchmark_suite(ctx: &GraphicsContext) -> Vec<Benchmark> {
    let ctx = BenchmarkSuiteContext::new(ctx);
    let vk_base = BenchmarkBuilder::new()
        .scenes([SINGLE_VIDEO_N_TO_N, TWO_VIDEO_2N_TO_N, FOUR_VIDEO_4N_TO_N])
        .encoders([EncoderOptions::VulkanH264])
        .decoders(vec![VideoDecoderOptions::VulkanH264]);
    let ffmpeg_base = BenchmarkBuilder::new()
        .scenes([SINGLE_VIDEO_N_TO_N, TWO_VIDEO_2N_TO_N, FOUR_VIDEO_4N_TO_N])
        .encoders([
            EncoderOptions::FfmpegH264(FfmpegH264EncoderPreset::Ultrafast),
            EncoderOptions::FfmpegH264(FfmpegH264EncoderPreset::Veryfast),
            EncoderOptions::FfmpegH264(FfmpegH264EncoderPreset::Fast),
        ])
        .decoders(vec![VideoDecoderOptions::FfmpegH264]);

    [
        // Common realistic scenarios
        vk_base
            .clone()
            .input_source([
                ctx.bbb_mp4_1080p30fps.clone(),
                ctx.bbb_mp4_2160p30fps.clone(),
            ])
            .output_resolutions([ResolutionPreset::Res2160p])
            .build(),
        vk_base
            .clone()
            .input_source([ctx.bbb_mp4_1080p30fps.clone()])
            .output_resolutions([ResolutionPreset::Res1440p])
            .build(),
        vk_base
            .clone()
            .input_source([ctx.bbb_mp4_720p24fps.clone()])
            .output_resolutions([ResolutionPreset::Res1080p])
            .build(),
        // Comparison against software based
        vk_base
            .clone()
            .input_source([ctx.bbb_mp4_1080p30fps.clone()])
            .output_resolutions([ResolutionPreset::Res1080p])
            .build(),
        vk_base
            .clone()
            .input_source([ctx.bbb_mp4_720p24fps.clone()])
            .output_resolutions([ResolutionPreset::Res720p])
            .build(),
        ffmpeg_base
            .clone()
            .input_source([ctx.bbb_mp4_1080p30fps.clone()])
            .output_resolutions([ResolutionPreset::Res1080p])
            .build(),
        ffmpeg_base
            .clone()
            .input_source([ctx.bbb_mp4_720p24fps.clone()])
            .output_resolutions([ResolutionPreset::Res720p])
            .build(),
    ]
    .concat()
}

pub fn vulkan_only_benchmark_suite(ctx: &GraphicsContext) -> Vec<Benchmark> {
    let ctx = BenchmarkSuiteContext::new(ctx);
    let base = BenchmarkBuilder::new()
        .scenes([SINGLE_VIDEO_N_TO_N, FOUR_VIDEO_4N_TO_N])
        .encoders([EncoderOptions::VulkanH264])
        .decoders(vec![VideoDecoderOptions::VulkanH264]);

    [
        base.clone()
            .input_source([
                ctx.bbb_mp4_1080p30fps.clone(),
                ctx.bbb_mp4_2160p30fps.clone(),
            ])
            .output_resolutions([ResolutionPreset::Res2160p])
            .build(),
        base.clone()
            .input_source([
                ctx.bbb_mp4_720p24fps.clone(),
                ctx.bbb_mp4_1080p30fps.clone(),
            ])
            .output_resolutions([ResolutionPreset::Res1080p, ResolutionPreset::Res1440p])
            .build(),
        base.clone()
            .input_source([ctx.bbb_mp4_720p24fps.clone()])
            .output_resolutions([ResolutionPreset::Res720p])
            .build(),
    ]
    .concat()
}

fn benchmark_set_constant_input_output_ratio() -> BenchmarkBuilder {
    BenchmarkBuilder::new()
        .scenes([SINGLE_VIDEO_N_TO_N, TWO_VIDEO_2N_TO_N, FOUR_VIDEO_4N_TO_N])
        .encoders([
            EncoderOptions::VulkanH264,
            EncoderOptions::FfmpegH264(FfmpegH264EncoderPreset::Ultrafast),
            EncoderOptions::FfmpegH264(FfmpegH264EncoderPreset::Veryfast),
            EncoderOptions::FfmpegH264(FfmpegH264EncoderPreset::Fast),
        ])
        .decoders(vec![
            VideoDecoderOptions::VulkanH264,
            VideoDecoderOptions::FfmpegH264,
        ])
}

fn benchmark_set_renderer_only(ctx: &'static BenchmarkSuiteContext) -> BenchmarkBuilder {
    BenchmarkBuilder::new()
        .id_prefix("rendering only")
        .scenes([
            BLANK_1_TO_N,
            TILES_1_TO_N,
            SINGLE_VIDEO_1_TO_N,
            // TODO: breaks xwayland (registers 1000 outputs)
            PASS_THROUGH_1_TO_N,
            STATIC_IMAGE_1_TO_N,
            IMAGE_WITH_SHADER_1_TO_N,
        ])
        .encoders([EncoderOptions::Disabled])
        .input_source([ctx.bbb_raw_720p_input.clone()])
        .output_resolutions([ResolutionPreset::Res720p])
}

fn benchmark_set_decoder_only(ctx: &'static BenchmarkSuiteContext) -> BenchmarkBuilder {
    BenchmarkBuilder::new()
        .id_prefix("decoding only")
        .scenes([BLANK_N_TO_1])
        .encoders([EncoderOptions::Disabled])
        .decoders(vec![
            VideoDecoderOptions::VulkanH264,
            VideoDecoderOptions::FfmpegH264,
        ])
        .input_source([
            ctx.bbb_mp4_480p24fps.clone(),
            ctx.bbb_mp4_720p24fps.clone(),
            ctx.bbb_mp4_1080p30fps.clone(),
            ctx.bbb_mp4_1080p60fps_no_audio.clone(),
            ctx.bbb_mp4_2160p30fps.clone(),
        ])
        .output_resolutions([ResolutionPreset::Res144p])
}

fn benchmark_set_encoder_only(ctx: &'static BenchmarkSuiteContext) -> BenchmarkBuilder {
    BenchmarkBuilder::new()
        .id_prefix("encoding only")
        .scenes([SINGLE_VIDEO_1_TO_N])
        .input_source([ctx.bbb_raw_720p_input.clone()])
        .encoders(vec![
            EncoderOptions::VulkanH264,
            EncoderOptions::FfmpegH264(FfmpegH264EncoderPreset::Fast),
            EncoderOptions::FfmpegH264(FfmpegH264EncoderPreset::Ultrafast),
        ])
        .output_resolutions([
            ResolutionPreset::Res480p,
            ResolutionPreset::Res720p,
            ResolutionPreset::Res1080p,
            ResolutionPreset::Res2160p,
        ])
}

// ---------------------------------------------------------------------------
// Utilities
// ---------------------------------------------------------------------------

struct BenchmarkSuiteContext {
    #[allow(dead_code)]
    wgpu_ctx: GraphicsContext,
    bbb_mp4_480p24fps: InputFile,
    bbb_mp4_720p24fps: InputFile,
    bbb_mp4_1080p30fps: InputFile,
    bbb_mp4_1080p60fps_no_audio: InputFile,
    bbb_mp4_2160p30fps: InputFile,
    bbb_raw_720p_input: InputFile,
}

impl BenchmarkSuiteContext {
    fn new(wgpu_ctx: &GraphicsContext) -> &'static Self {
        let bbb_mp4_480p24fps_path = ensure_bunny_480p24fps().unwrap();
        let bbb_mp4_720p24fps_path = ensure_bunny_720p24fps().unwrap();
        let bbb_mp4_1080p30fps_path = ensure_bunny_1080p30fps().unwrap();
        let bbb_mp4_1080p60fps_path = ensure_bunny_1080p60fps_no_audio().unwrap();
        let bbb_mp4_2160p30fps_path = ensure_bunny_2160p30fps().unwrap();

        generate_png_from_video(&bbb_mp4_1080p30fps_path);
        let raw_720p = generate_yuv_from_mp4(&bbb_mp4_720p24fps_path).unwrap();

        Box::leak(
            Self {
                wgpu_ctx: wgpu_ctx.clone(),
                bbb_mp4_480p24fps: InputFile {
                    label: "bbb_mp4_480p24fps",
                    kind: InputFileKind::Mp4(bbb_mp4_480p24fps_path),
                },
                bbb_mp4_720p24fps: InputFile {
                    label: "bbb_mp4_720p24fps",
                    kind: InputFileKind::Mp4(bbb_mp4_720p24fps_path),
                },
                bbb_mp4_1080p30fps: InputFile {
                    label: "bbb_mp4_1080p30fps",
                    kind: InputFileKind::Mp4(bbb_mp4_1080p30fps_path),
                },
                bbb_mp4_1080p60fps_no_audio: InputFile {
                    label: "bbb_mp4_1080p60fps_no_audio",
                    kind: InputFileKind::Mp4(bbb_mp4_1080p60fps_path),
                },
                bbb_mp4_2160p30fps: InputFile {
                    label: "bbb_mp4_2160p30fps",
                    kind: InputFileKind::Mp4(bbb_mp4_2160p30fps_path),
                },
                bbb_raw_720p_input: InputFile {
                    label: "raw_720p",
                    kind: InputFileKind::Raw(raw_720p),
                },
            }
            .into(),
        )
    }
}

macro_rules! cartesian_product {
    ($($list:expr),+ $(,)?) => {{
        let mut result = Vec::new();
        cartesian_product!(
            @iter result,
            [_v0 _v1 _v2 _v3 _v4 _v5 _v6 _v7],
            [],
            [$($list),+]
        );
        result
    }};
    (@iter $result:ident, [$name:ident $($rest:ident)*], [$($bound:ident)*], [$head:expr]) => {
        for $name in $head.iter() {
            $result.push(($($bound.clone(),)* $name.clone(),));
        }
    };
    (@iter $result:ident, [$name:ident $($rest:ident)*], [$($bound:ident)*], [$head:expr, $($tail:expr),+]) => {
        for $name in $head.iter() {
            cartesian_product!(@iter $result, [$($rest)*], [$($bound)* $name], [$($tail),+]);
        }
    };
}

#[derive(Clone)]
struct BenchmarkBuilder {
    id_prefix: Option<&'static str>,
    scenes: Vec<SceneLayout>,
    encoders: Vec<EncoderOptions>,
    decoders: Vec<VideoDecoderOptions>,
    input_source: Vec<InputFile>,
    output_resolutions: Vec<ResolutionPreset>,
    rendering_mode: Option<RenderingMode>,
}

impl BenchmarkBuilder {
    fn new() -> Self {
        Self {
            id_prefix: None,
            scenes: Vec::new(),
            encoders: Vec::new(),
            decoders: Vec::new(),
            input_source: Vec::new(),
            output_resolutions: Vec::new(),
            rendering_mode: None,
        }
    }

    fn id_prefix(mut self, prefix: &'static str) -> Self {
        self.id_prefix = Some(prefix);
        self
    }

    fn rendering_mode(mut self, mode: RenderingMode) -> Self {
        self.rendering_mode = Some(mode);
        self
    }

    fn scenes(mut self, scenes: impl IntoIterator<Item = SceneLayout>) -> Self {
        self.scenes = scenes.into_iter().collect();
        self
    }

    fn encoders(mut self, encoders: impl IntoIterator<Item = EncoderOptions>) -> Self {
        self.encoders = encoders.into_iter().collect();
        self
    }

    fn decoders(mut self, decoders: impl IntoIterator<Item = VideoDecoderOptions>) -> Self {
        self.decoders = decoders.into_iter().collect();
        self
    }

    fn input_source(mut self, inputs: impl IntoIterator<Item = InputFile>) -> Self {
        self.input_source = inputs.into_iter().collect();
        self
    }

    fn output_resolutions(
        mut self,
        resolutions: impl IntoIterator<Item = ResolutionPreset>,
    ) -> Self {
        self.output_resolutions = resolutions.into_iter().collect();
        self
    }

    fn build(self) -> Vec<Benchmark> {
        let scenes = wrap_axis(self.scenes);
        let encoders = wrap_axis(self.encoders);
        let decoders = wrap_axis(self.decoders);
        let inputs = wrap_axis(self.input_source);
        let resolutions = wrap_axis(self.output_resolutions);
        let id_prefix = self.id_prefix;
        let rendering_mode = self.rendering_mode;

        cartesian_product!(&scenes, &encoders, &decoders, &inputs, &resolutions)
            .into_iter()
            .map(|(scene, encoder, decoder, input, resolution)| {
                let mut id_parts: Vec<String> = Vec::new();
                if let Some(prefix) = id_prefix {
                    id_parts.push(prefix.to_string());
                }
                if let Some(s) = &scene {
                    id_parts.push(s.label.to_string());
                }
                if let Some(e) = &encoder {
                    id_parts.push(format!("encoder: {e:?}"));
                }
                if let Some(d) = &decoder {
                    id_parts.push(format!("decoder: {d:?}"));
                }
                if let Some(i) = &input {
                    id_parts.push(format!("input: {}", i.label));
                }
                if let Some(r) = &resolution {
                    id_parts.push(format!("output: {r:?}"));
                }
                let id: &'static str = id_parts.join(" - ").leak();

                Benchmark {
                    id,
                    bench_pass_builder: Arc::new(Box::new(move |value: u64| {
                        let (builder, input_count, output_count, resources): (
                            BuilderFn,
                            u64,
                            u64,
                            Vec<_>,
                        ) = match &scene {
                            Some(s) => (
                                s.builder,
                                s.inputs.eval(value),
                                s.outputs.eval(value),
                                (s.resources)(),
                            ),
                            None => (TILES_1_TO_N.builder, value, value, vec![]),
                        };

                        SingleBenchmarkPass {
                            builder,
                            resources,
                            input_count,
                            output_count,
                            framerate: 30,
                            output_resolution: resolution
                                .unwrap_or(ResolutionPreset::Res1080p)
                                .into(),
                            input_file: input.clone().unwrap_or(InputFile {
                                label: "placeholder",
                                kind: InputFileKind::Mp4(PathBuf::new()),
                            }),
                            encoder: encoder.unwrap_or(EncoderOptions::FfmpegH264(
                                FfmpegH264EncoderPreset::Ultrafast,
                            )),
                            decoder: decoder.unwrap_or(VideoDecoderOptions::FfmpegH264),
                            warm_up_time: Duration::from_secs(2),
                            rendering_mode: rendering_mode.unwrap_or(RenderingMode::GpuOptimized),
                        }
                    })),
                }
            })
            .collect()
    }
}

fn wrap_axis<T>(values: Vec<T>) -> Vec<Option<T>> {
    if values.is_empty() {
        vec![None]
    } else {
        values.into_iter().map(Some).collect()
    }
}
