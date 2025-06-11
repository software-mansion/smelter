use std::{fs, sync::Arc, time::Duration};

use benchmark::{Benchmark, EncoderOptions};
use benchmark_pass::{InputFile, SingleBenchmarkPass};
use clap::Parser;
use compositor_pipeline::pipeline::{GraphicsContext, GraphicsContextOptions};

use compositor_render::RenderingMode;
use scenes::simple_tiles_with_all_inputs;
use smelter::{
    config::{read_config, LoggerConfig},
    logger,
};
use suite::{cpu_optimized_benchmark_suite, full_benchmark_suite, minimal_benchmark_suite};
use tracing::{info, warn};

mod args;
mod benchmark;
mod benchmark_pass;
mod maximize_iter;
mod scenes;
mod suite;
mod utils;

use args::{Args, BenchmarkSuite, NumericArgument, Resolution, ResolutionArgument};
use utils::{ensure_default_mp4, generate_yuv_from_mp4};

fn main() {
    let args = Args::parse();
    let config = read_config();
    ffmpeg_next::format::network::init();
    let logger_config = LoggerConfig {
        level: "compositor_pipeline=error,vk-video=info,benchmark=info".into(),
        ..config.logger
    };
    logger::init_logger(logger_config);

    let ctx = GraphicsContext::new(GraphicsContextOptions {
        features: wgpu::Features::SAMPLED_TEXTURE_AND_STORAGE_BUFFER_ARRAY_NON_UNIFORM_INDEXING,
        ..Default::default()
    })
    .unwrap();

    if cfg!(debug_assertions) {
        warn!("This benchmark is running in debug mode. Make sure to run in release mode for reliable results.");
    }

    let benchmarks = match args.suite {
        BenchmarkSuite::Full => full_benchmark_suite(&ctx),
        BenchmarkSuite::CpuOptimized => cpu_optimized_benchmark_suite(&ctx),
        BenchmarkSuite::Minimal => minimal_benchmark_suite(&ctx),
        BenchmarkSuite::None => benchmark_from_args(args.clone()),
    };

    let results: Vec<_> = benchmarks
        .iter()
        .map(|benchmark| {
            info!("benchmark: {}", benchmark.id);
            benchmark.run(&ctx)
        })
        .collect();

    if args.json {
        let value = serde_json::Value::Array(results.iter().map(|r| r.json()).collect());
        println!("{}", serde_json::to_string_pretty(&value).unwrap())
    } else {
        for result in &results {
            println!("{}", result.text())
        }
    }

    if let Some(path) = args.json_file {
        let value = serde_json::Value::Array(results.iter().map(|r| r.json()).collect());
        fs::write(path, serde_json::to_string_pretty(&value).unwrap()).unwrap()
    };
}

fn benchmark_from_args(args: Args) -> Vec<Benchmark> {
    let input_path = args
        .input_path
        .clone()
        .unwrap_or_else(|| ensure_default_mp4().unwrap());
    [Benchmark {
        id: "from_args",
        bench_pass_builder: Arc::new(Box::new(move |value: u64| {
            let (input_count, output_count, framerate, output_resolution) = match (
                args.input_count,
                args.output_count,
                args.framerate,
                args.output_resolution,
            ) {
                (
                    NumericArgument::Maximize,
                    NumericArgument::Constant(output_count),
                    NumericArgument::Constant(framerate),
                    ResolutionArgument::Constant(output_resolution),
                ) => (value, output_count, framerate, output_resolution),
                (
                    NumericArgument::Constant(input_count),
                    NumericArgument::Maximize,
                    NumericArgument::Constant(framerate),
                    ResolutionArgument::Constant(output_resolution),
                ) => (input_count, value, framerate, output_resolution),
                (
                    NumericArgument::Constant(input_count),
                    NumericArgument::Constant(output_count),
                    NumericArgument::Maximize,
                    ResolutionArgument::Constant(output_resolution),
                ) => (input_count, output_count, value, output_resolution),
                (
                    NumericArgument::Constant(input_count),
                    NumericArgument::Constant(output_count),
                    NumericArgument::Constant(framerate),
                    ResolutionArgument::Maximize,
                ) => (
                    input_count,
                    output_count,
                    framerate,
                    Resolution {
                        width: 256 * value as usize,
                        height: 144 * value as usize,
                    },
                ),
                (_, _, _, _) => panic!("Exactly one maximize is required"),
            };

            SingleBenchmarkPass {
                scene_builder: simple_tiles_with_all_inputs,
                resources: vec![],

                input_count,
                output_count,
                framerate,
                output_resolution,

                input_file: match args.disable_decoder {
                    true => InputFile::Raw(generate_yuv_from_mp4(&input_path).unwrap()),
                    false => InputFile::Mp4(input_path.clone()),
                },
                encoder: match args.disable_encoder {
                    true => EncoderOptions::Disabled,
                    false => EncoderOptions::Enabled(args.encoder_preset.into()),
                },
                decoder: args.video_decoder.into(),

                warm_up_time: Duration::from_secs(2),
                rendering_mode: RenderingMode::GpuOptimized,
            }
        })),
    }]
    .into()
}
