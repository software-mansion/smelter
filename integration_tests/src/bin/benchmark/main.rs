use std::fs;

use benchmark::{Benchmark, EncoderOptions};
use benchmark_pass::InputFile;
use clap::Parser;
use compositor_pipeline::pipeline::{GraphicsContext, GraphicsContextOptions};

use scenes::simple_tiles_with_all_inputs;
use smelter::{
    config::{read_config, LoggerConfig},
    logger,
};
use suite::{full_benchmark_suite, minimal_benchmark_suite};
use tracing::{info, warn};

mod args;
mod benchmark;
mod benchmark_pass;
mod maximize_iter;
mod scenes;
mod suite;
mod utils;

use args::{Args, BenchmarkSuite};
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
        features: wgpu::Features::SAMPLED_TEXTURE_AND_STORAGE_BUFFER_ARRAY_NON_UNIFORM_INDEXING
            | wgpu::Features::UNIFORM_BUFFER_AND_STORAGE_TEXTURE_ARRAY_NON_UNIFORM_INDEXING,
        ..Default::default()
    })
    .unwrap();

    if cfg!(debug_assertions) {
        warn!("This benchmark is running in debug mode. Make sure to run in release mode for reliable results.");
    }

    let benchmarks = match args.suite {
        BenchmarkSuite::Full => full_benchmark_suite(),
        BenchmarkSuite::Minimal => minimal_benchmark_suite(),
        BenchmarkSuite::None => benchmark_from_args(&args),
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

fn benchmark_from_args(args: &Args) -> Vec<Benchmark> {
    let input_path = args
        .input_path
        .clone()
        .unwrap_or_else(|| ensure_default_mp4().unwrap());
    [Benchmark {
        id: "from_args",

        scene_builder: simple_tiles_with_all_inputs,

        input_count: args.input_count.into(),
        output_count: args.output_count.into(),
        framerate: args.framerate.into(),
        output_resolution: args.output_resolution.into(),

        input_file: match args.disable_decoder {
            true => InputFile::Raw(generate_yuv_from_mp4(&input_path).unwrap()),
            false => InputFile::Mp4(input_path.clone()),
        },
        encoder: match args.disable_encoder {
            true => EncoderOptions::Disabled,
            false => EncoderOptions::Enabled(args.encoder_preset.into()),
        },
        decoder: args.video_decoder.into(),

        warm_up_time: args.warm_up_time.0,
        measure_time: args.measure_time.0,
        error_tolerance_multiplier: args.error_tolerance,
    }]
    .into()
}
