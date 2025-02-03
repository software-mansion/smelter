use std::{
    sync::{mpsc, Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use clap::Parser;
use compositor_pipeline::{
    pipeline::{
        encoder::VideoEncoderOptions,
        input::{
            mp4::{Mp4Options, Source},
            InputOptions,
        },
        output::EncodedDataOutputOptions,
        GraphicsContext, Options, OutputVideoOptions, PipelineOutputEndCondition,
        RegisterInputOptions, RegisterOutputOptions,
    },
    queue::{self, QueueInputOptions, QueueOptions},
    Pipeline,
};

use compositor_pipeline::pipeline::encoder::ffmpeg_h264::Options as H264OutputOptions;
use compositor_render::{
    scene::{
        Component, HorizontalAlign, InputStreamComponent, RGBAColor, TilesComponent, VerticalAlign,
    },
    web_renderer::WebRendererInitOptions,
    Framerate, InputId, OutputId, Resolution,
};
use smelter::{
    config::{read_config, LoggerConfig},
    logger,
};
use tracing::warn;

mod args;

use args::{Args, Argument, SingleBenchConfig};

fn main() {
    let args = Args::parse();
    let config = read_config();
    ffmpeg_next::format::network::init();
    let logger_config = LoggerConfig {
        level: "compositor_pipeline=error,vk-video=info,benchmark=info".into(),
        ..config.logger
    };
    logger::init_logger(logger_config);

    let ctx = GraphicsContext::new(
        false,
        wgpu::Features::SAMPLED_TEXTURE_AND_STORAGE_BUFFER_ARRAY_NON_UNIFORM_INDEXING
            | wgpu::Features::UNIFORM_BUFFER_AND_STORAGE_TEXTURE_ARRAY_NON_UNIFORM_INDEXING,
        Default::default(),
        None,
    )
    .unwrap();

    if cfg!(debug_assertions) {
        warn!("This benchmark is running in debug mode. Make sure to run in release mode for reliable results.");
    }

    let reports = run_args(ctx, &args);
    SingleBenchConfig::log_report_header();
    for report in reports {
        report.log_as_report();
    }
}

fn run_args(ctx: GraphicsContext, args: &Args) -> Vec<SingleBenchConfig> {
    let arguments = args.arguments();
    let mut reports = Vec::new();

    // check maximize count
    let maximize_count = arguments
        .iter()
        .filter(|arg| matches!(arg, Argument::Maximize))
        .count();

    if maximize_count != 1 {
        println!("Exactly one argument should be set to 'maximize'");
        return Vec::new();
    }

    run_args_iterate(ctx, args, arguments, &mut reports);

    reports
}

struct FurtherIterationPossible(bool);

fn run_args_iterate(
    ctx: GraphicsContext,
    args: &Args,
    arguments: Box<[Argument]>,
    reports: &mut Vec<SingleBenchConfig>,
) -> FurtherIterationPossible {
    for (i, argument) in arguments.iter().enumerate() {
        if matches!(argument, Argument::IterateExp) {
            let mut any_succeeded = false;
            let mut count = 1;

            // run the rest of the benchmark, multiplying the argument by 2 each iteration
            loop {
                let mut arguments = arguments.clone();
                arguments[i] = Argument::Constant(count);

                if let FurtherIterationPossible(true) =
                    run_args_iterate(ctx.clone(), args, arguments, reports)
                {
                    any_succeeded = true;
                    count *= 2;
                    continue;
                } else {
                    // If no benchmarks finished successfully, even with the argument set to 1, we
                    // have to tell the previous recurrent invocation of this function that the configuration
                    // it gave us is too hard to run already and that it can stop iterating,
                    // because it has reached the maximum for its argument.
                    //
                    // If some benchmarks finished successfully, then the previous recurrent
                    // invocation can increase its arguments again, until we get to the situation
                    // where no further iteration is possible.
                    return FurtherIterationPossible(any_succeeded);
                }
            }
        }
    }

    // If the for loop above didn't run at all, then all arguments are either Constant or Maximize,
    // so we can run the maximization
    run_args_maximize(ctx, args, arguments, reports)
}

fn run_args_maximize(
    ctx: GraphicsContext,
    args: &Args,
    arguments: Box<[Argument]>,
    reports: &mut Vec<SingleBenchConfig>,
) -> FurtherIterationPossible {
    let test_fn = |count, i| {
        let mut arguments = arguments.clone();
        arguments[i] = Argument::Constant(count);
        let config = args.with_arguments(&arguments);
        config.log_running_config();
        run_single_test(ctx.clone(), config)
    };

    for (i, argument) in arguments.iter().enumerate() {
        if *argument == Argument::Maximize {
            let upper_bound = find_upper_bound(1, |count| test_fn(count, i));

            if upper_bound == 0 {
                // the configuration is not runnable anymore
                return FurtherIterationPossible(false);
            }

            let result = binsearch(upper_bound / 2, upper_bound, |count| test_fn(count, i));

            let mut arguments = arguments.clone();
            arguments[i] = Argument::Constant(result);
            reports.push(args.with_arguments(&arguments));
            return FurtherIterationPossible(true);
        }
    }

    unreachable!("There should be an argument set to maximize.");
}

fn binsearch(mut start: u64, mut end: u64, test_fn: impl Fn(u64) -> bool) -> u64 {
    while start < end {
        let midpoint = (start + end + 1) / 2;

        if test_fn(midpoint) {
            start = midpoint;
        } else {
            end = midpoint - 1;
        }
    }

    end
}

fn find_upper_bound(start: u64, test_fn: impl Fn(u64) -> bool) -> u64 {
    let mut end = start;

    while test_fn(end) {
        end *= 2;
    }

    end - 1
}

/// true - works
/// false - too slow
fn run_single_test(ctx: GraphicsContext, bench_config: SingleBenchConfig) -> bool {
    let pipeline_result = Pipeline::new(Options {
        queue_options: QueueOptions {
            never_drop_output_frames: true,
            output_framerate: Framerate {
                num: bench_config.framerate as u32,
                den: 1,
            },
            default_buffer_duration: queue::DEFAULT_BUFFER_DURATION,
            ahead_of_time_processing: false,
            run_late_scheduled_events: true,
        },
        web_renderer: WebRendererInitOptions {
            enable: false,
            enable_gpu: false,
        },
        wgpu_ctx: Some(ctx),
        force_gpu: false,
        download_root: std::env::temp_dir(),
        wgpu_features: wgpu::Features::UNIFORM_BUFFER_AND_STORAGE_TEXTURE_ARRAY_NON_UNIFORM_INDEXING
            | wgpu::Features::SAMPLED_TEXTURE_AND_STORAGE_BUFFER_ARRAY_NON_UNIFORM_INDEXING,
        load_system_fonts: Some(false),
        mixing_sample_rate: 48_000,
        stream_fallback_timeout: Duration::from_millis(500),
        tokio_rt: None,
        stun_servers: Vec::new().into(),
        whip_whep_server_port: None,
        start_whip_whep: false,
    });

    let Ok((pipeline, _event_loop)) = pipeline_result else {
        return false;
    };

    let pipeline = Arc::new(Mutex::new(pipeline));

    let mut inputs = Vec::new();
    for i in 0..bench_config.decoder_count {
        let input_id = InputId(format!("input_{i}").into());
        inputs.push(input_id.clone());

        let result = Pipeline::register_input(
            &pipeline,
            input_id,
            RegisterInputOptions {
                input_options: InputOptions::Mp4(Mp4Options {
                    should_loop: true,
                    video_decoder: bench_config.video_decoder,
                    source: Source::File(bench_config.file_path.clone()),
                }),
                queue_options: QueueInputOptions {
                    offset: Some(Duration::ZERO),
                    required: true,
                    buffer_duration: None,
                },
            },
        );

        if result.is_err() {
            return false;
        }
    }

    let mut receivers = Vec::new();
    for i in 0..bench_config.encoder_count {
        let output_id = OutputId(format!("output_{i}").into());
        let receiver_result = Pipeline::register_encoded_data_output(
            &pipeline,
            output_id,
            RegisterOutputOptions {
                video: Some(OutputVideoOptions {
                    end_condition: PipelineOutputEndCondition::AnyInput,
                    initial: Component::Tiles(TilesComponent {
                        id: None,
                        width: Some(bench_config.output_resolution.width as f32),
                        height: Some(bench_config.output_resolution.height as f32),
                        margin: 2.0,
                        padding: 0.0,
                        children: inputs
                            .clone()
                            .into_iter()
                            .map(|i| {
                                Component::InputStream(InputStreamComponent {
                                    id: None,
                                    input_id: i,
                                })
                            })
                            .collect(),
                        transition: None,
                        vertical_align: VerticalAlign::Center,
                        horizontal_align: HorizontalAlign::Center,
                        background_color: RGBAColor(128, 128, 128, 0),
                        tile_aspect_ratio: (16, 9),
                    }),
                }),

                audio: None,
                output_options: EncodedDataOutputOptions {
                    audio: None,
                    video: Some(VideoEncoderOptions::H264(H264OutputOptions {
                        preset: bench_config.output_encoder_preset.clone(),
                        resolution: Resolution {
                            width: bench_config.output_resolution.width as usize,
                            height: bench_config.output_resolution.height as usize,
                        },
                        raw_options: Vec::new(),
                    })),
                },
            },
        );

        let Ok(pipeline_receiver) = receiver_result else {
            return false;
        };

        receivers.push(pipeline_receiver);
    }

    let (result_sender, result_receiver) = mpsc::channel::<bool>();

    Pipeline::start(&pipeline);
    for pipeline_receiver in receivers {
        let result_sender_clone = result_sender.clone();
        thread::spawn(move || {
            let start_time = Instant::now();
            while start_time.elapsed() < bench_config.warm_up_time {
                _ = pipeline_receiver.recv().unwrap();
            }

            let start_time = Instant::now();
            let mut produced_frames: usize = 0;
            while start_time.elapsed() < bench_config.measured_time {
                _ = pipeline_receiver.recv().unwrap();
                produced_frames += 1;
            }

            let end_time = Instant::now();

            let framerate = produced_frames as f64 / (end_time - start_time).as_secs_f64();

            let _ = result_sender_clone.send(
                framerate * bench_config.framerate_tolerance_multiplier
                    > bench_config.framerate as f64,
            );
        });
    }
    for _ in 0..bench_config.encoder_count {
        match result_receiver.recv() {
            Ok(test_result) => {
                if !test_result {
                    return false;
                }
            }
            Err(_) => return false,
        }
    }
    true
}
