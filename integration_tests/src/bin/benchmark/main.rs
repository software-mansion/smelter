use std::{
    fs::File,
    io::Read,
    sync::{mpsc, Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use clap::Parser;
use compositor_pipeline::{
    error::RegisterInputError,
    pipeline::{
        encoder::VideoEncoderOptions,
        input::{
            mp4::{Mp4Options, Source},
            InputInitInfo, InputOptions, RawDataInputOptions,
        },
        output::{EncodedDataOutputOptions, RawDataOutputOptions, RawVideoOptions},
        GraphicsContext, GraphicsContextOptions, Options, OutputVideoOptions,
        PipelineOutputEndCondition, RegisterInputOptions, RegisterOutputOptions,
    },
    queue::{self, PipelineEvent, QueueInputOptions, QueueOptions},
    Pipeline,
};

use compositor_pipeline::pipeline::encoder::ffmpeg_h264::Options as H264OutputOptions;
use compositor_render::{
    scene::{
        Component, HorizontalAlign, InputStreamComponent, RGBAColor, TilesComponent, VerticalAlign,
    },
    web_renderer::WebRendererInitOptions,
    Frame, Framerate, InputId, OutputId, Resolution, YuvPlanes,
};
use crossbeam_channel::{Receiver, Sender};
use smelter::{
    config::{read_config, LoggerConfig},
    logger,
};
use tracing::warn;

mod args;

use args::{
    Args, Argument, CsvWriter, ExpIterator, NumericArgument, ResolutionArgument, ResolutionPreset,
    SingleBenchConfig,
};

trait PipelineReceiver {
    fn receive(&self) {}
}

impl<T> PipelineReceiver for Receiver<T> {
    fn receive(&self) {
        let _ = self.recv();
    }
}

fn main() {
    let mut args = Args::parse();
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

    if args.disable_decoder {
        let time = args.measured_time.0 + args.warm_up_time.0;
        convert_mp4_to_yuv(&mut args, time).unwrap();
    }

    let mut csv_writer = args.csv_path.clone().map(CsvWriter::init);

    let reports = run_args(ctx, &args);
    SingleBenchConfig::log_report_header(&mut csv_writer);
    for report in reports {
        report.log_as_report(csv_writer.as_mut());
    }
}

fn convert_mp4_to_yuv(args: &mut Args, duration: Duration) -> Result<(), String> {
    let output_path = std::path::PathBuf::from(format!(
        "/tmp/smelter_benchmark_input_{}.yuv",
        std::time::UNIX_EPOCH.elapsed().unwrap().as_millis()
    ));

    let mut probe_cmd = std::process::Command::new("ffprobe");
    probe_cmd
        .arg("-v")
        .arg("error")
        .arg("-select_streams")
        .arg("v:0")
        .arg("-show_entries")
        .arg("stream=width,height,r_frame_rate")
        .arg("-of")
        .arg("csv=p=0")
        .arg(&args.file_path)
        .stdout(std::process::Stdio::piped());

    let probe_result = probe_cmd
        .spawn()
        .map_err(|err| err.to_string())?
        .wait_with_output()
        .map_err(|err| err.to_string())?;

    if !probe_result.status.success() {
        return Err("ffprobe command failed".into());
    }

    let probe_result = String::from_utf8(probe_result.stdout).unwrap();
    let mut probe_parts = probe_result.trim().split(",");
    let width = probe_parts.next().unwrap().parse::<usize>().unwrap();
    let height = probe_parts.next().unwrap().parse::<usize>().unwrap();
    let mut framerate = probe_parts.next().unwrap().split("/");
    let num = framerate.next().unwrap().parse::<u32>().unwrap();
    let den = framerate.next().unwrap().parse::<u32>().unwrap();

    let mut convert_cmd = std::process::Command::new("ffmpeg");
    convert_cmd
        .arg("-i")
        .arg(&args.file_path)
        .arg("-pix_fmt")
        .arg("yuv420p")
        .arg("-t")
        .arg((duration.mul_f64(1.5)).as_secs().to_string())
        .arg(&output_path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());

    let mut ffmpeg = convert_cmd.spawn().map_err(|e| e.to_string())?;
    let status = ffmpeg
        .wait()
        .expect("wait for ffmpeg to finish yuv conversion");

    if !status.success() {
        return Err("ffmpeg for yuv conversion terminated unsuccessfully".into());
    }

    args.file_path = output_path.clone();
    args.input_options = Some(args::RawInputOptions {
        resolution: args::Resolution { width, height },
        framerate: num / den,
    });

    Ok(())
}

fn run_args(ctx: GraphicsContext, args: &Args) -> Vec<SingleBenchConfig> {
    let arguments = args.arguments();
    let mut reports = Vec::new();

    // check maximize count
    let maximize_count = arguments.iter().filter(|arg| arg.is_maximize()).count();

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
        match argument {
            Argument::NumericArgument(numeric_argument) => {
                if matches!(numeric_argument, NumericArgument::IterateExp) {
                    let iterator = ExpIterator::default()
                        .map(|v| Argument::NumericArgument(NumericArgument::Constant(v)));
                    return run_fn_iterate(ctx, reports, args, i, arguments, iterator);
                }
            }
            Argument::ResolutionArgument(resolution_argument) => {
                if matches!(resolution_argument, args::ResolutionArgument::Iterate) {
                    let iterator = ResolutionPreset::iter().map(|v| {
                        Argument::ResolutionArgument(ResolutionArgument::Constant(v.into()))
                    });
                    return run_fn_iterate(ctx, reports, args, i, arguments, iterator);
                }
            }
        }
    }

    // If the for loop above didn't run at all, then all arguments are either Constant or Maximize,
    // so we can run the maximization
    run_args_maximize(ctx, args, arguments, reports)
}

fn run_fn_iterate(
    ctx: GraphicsContext,
    reports: &mut Vec<SingleBenchConfig>,
    args: &Args,
    i: usize,
    arguments: Box<[Argument]>,
    iterator: impl Iterator<Item = Argument>,
) -> FurtherIterationPossible {
    let mut any_succeeded = false;

    // run the rest of the benchmark, multiplying the argument by 2 each iteration
    for argument in iterator {
        let mut arguments = arguments.clone();
        arguments[i] = argument;
        if let FurtherIterationPossible(true) =
            run_args_iterate(ctx.clone(), args, arguments, reports)
        {
            any_succeeded = true;
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
    FurtherIterationPossible(any_succeeded)
}
fn run_args_maximize(
    ctx: GraphicsContext,
    args: &Args,
    arguments: Box<[Argument]>,
    reports: &mut Vec<SingleBenchConfig>,
) -> FurtherIterationPossible {
    let test_fn = |argument, i| {
        let mut arguments = arguments.clone();
        arguments[i] = argument;
        let config = args.with_arguments(&arguments);
        config.log_running_config();
        run_single_test(ctx.clone(), config)
    };
    let to_numeric_const = |x| Argument::NumericArgument(NumericArgument::Constant(x));
    for (i, argument) in arguments.iter().enumerate() {
        match argument {
            Argument::NumericArgument(numeric_argument) => {
                if *numeric_argument == NumericArgument::Maximize {
                    {
                        let upper_bound =
                            find_upper_bound(1, |count| test_fn(to_numeric_const(count), i));

                        if upper_bound == 0 {
                            // the configuration is not runnable anymore
                            return FurtherIterationPossible(false);
                        }

                        let result = binsearch(upper_bound / 2, upper_bound, |count| {
                            test_fn(to_numeric_const(count), i)
                        });

                        let mut arguments = arguments.clone();
                        arguments[i] = Argument::NumericArgument(NumericArgument::Constant(result));
                        reports.push(args.with_arguments(&arguments));
                        return FurtherIterationPossible(true);
                    }
                }
            }
            Argument::ResolutionArgument(resolution_argument)
                if *resolution_argument == ResolutionArgument::Maximize =>
            {
                let iterator = ResolutionPreset::iter()
                    .map(|v| Argument::ResolutionArgument(ResolutionArgument::Constant(v.into())));
                for preset in iterator {
                    let mut arguments = arguments.clone();
                    arguments[i] = preset.clone();
                    if !test_fn(preset.clone(), i) {
                        let mut arguments = arguments.clone();
                        arguments[i] = preset;
                        reports.push(args.with_arguments(&arguments));
                        return FurtherIterationPossible(true);
                    }
                }
            }
            _ => {}
        }
    }

    unreachable!("There should be an argument set to maximize.");
}

fn binsearch(mut start: u64, mut end: u64, test_fn: impl Fn(u64) -> bool) -> u64 {
    while start < end {
        let midpoint = (start + end).div_ceil(2);

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
        whip_whep_server_port: 9000,
        start_whip_whep: false,
        rendering_mode: compositor_render::RenderingMode::GpuOptimized,
    });

    let Ok((pipeline, _event_loop)) = pipeline_result else {
        return false;
    };

    let pipeline = Arc::new(Mutex::new(pipeline));

    let mut inputs = Vec::new();
    let mut frame_senders = Vec::new();
    for i in 0..bench_config.input_count {
        let input_id = InputId(format!("input_{i}").into());
        inputs.push(input_id.clone());
        match bench_config.disable_decoder {
            true => {
                let Ok(pipeline_sender) = register_pipeline_raw_input(&pipeline, input_id) else {
                    return false;
                };
                frame_senders.push(pipeline_sender);
            }
            false => {
                if register_pipeline_mp4_input(&pipeline, input_id, &bench_config).is_err() {
                    return false;
                }
            }
        }
    }

    let mut receivers = Vec::new();
    for i in 0..bench_config.output_count {
        let output_id = OutputId(format!("output_{i}").into());
        let output_video_options = OutputVideoOptions {
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
        };
        let receiver_result: Result<Box<dyn PipelineReceiver + Send>, Box<dyn std::error::Error>> =
            match bench_config.disable_encoder {
                true => register_pipeline_raw_output(
                    &pipeline,
                    output_id,
                    output_video_options,
                    &bench_config,
                ),
                false => register_pipeline_encoded_output(
                    &pipeline,
                    output_id,
                    output_video_options,
                    &bench_config,
                ),
            };

        let Ok(pipeline_receiver) = receiver_result else {
            return false;
        };

        receivers.push(pipeline_receiver);
    }

    let (result_sender, result_receiver) = mpsc::channel::<bool>();

    Pipeline::start(&pipeline);
    if bench_config.disable_decoder {
        let config = bench_config.clone();
        thread::spawn(move || raw_data_sender(config, frame_senders));
    }
    for pipeline_receiver in receivers {
        let result_sender_clone = result_sender.clone();
        thread::spawn(move || {
            let start_time = Instant::now();
            while start_time.elapsed() < bench_config.warm_up_time {
                pipeline_receiver.receive();
            }

            let start_time = Instant::now();
            let mut produced_frames: usize = 0;
            while start_time.elapsed() < bench_config.measured_time {
                pipeline_receiver.receive();
                produced_frames += 1;
            }

            let framerate = produced_frames as f64 / (start_time.elapsed()).as_secs_f64();

            if let Err(err) = result_sender_clone.send(
                framerate * bench_config.framerate_tolerance_multiplier
                    > bench_config.framerate as f64,
            ) {
                warn!("Error while sending bench results: {}", err);
            }
        });
    }

    let mut successful = true;
    for _ in 0..bench_config.output_count {
        match result_receiver.recv() {
            Ok(test_result) => {
                if !test_result {
                    successful = false;
                }
            }
            Err(err) => {
                warn!("Error while receiving bench results: {}", err);
                return false;
            }
        }
    }

    successful
}

fn register_pipeline_encoded_output(
    pipeline: &Arc<Mutex<Pipeline>>,
    output_id: OutputId,
    output_video_options: OutputVideoOptions,
    bench_config: &SingleBenchConfig,
) -> Result<Box<dyn PipelineReceiver + Send>, Box<dyn std::error::Error>> {
    let preset = bench_config.output_encoder_preset.clone();
    Ok(Box::new(Pipeline::register_encoded_data_output(
        pipeline,
        output_id,
        RegisterOutputOptions {
            video: Some(output_video_options),

            audio: None,
            output_options: EncodedDataOutputOptions {
                audio: None,
                video: Some(VideoEncoderOptions::H264(H264OutputOptions {
                    preset,
                    resolution: Resolution {
                        width: bench_config.output_resolution.width,
                        height: bench_config.output_resolution.height,
                    },
                    raw_options: Vec::new(),
                })),
            },
        },
    )?))
}

fn register_pipeline_raw_output(
    pipeline: &Arc<Mutex<Pipeline>>,
    output_id: OutputId,
    output_video_options: OutputVideoOptions,
    bench_config: &SingleBenchConfig,
) -> Result<Box<dyn PipelineReceiver + Send>, Box<dyn std::error::Error>> {
    let x = Pipeline::register_raw_data_output(
        pipeline,
        output_id,
        RegisterOutputOptions {
            video: Some(output_video_options),

            audio: None,
            output_options: RawDataOutputOptions {
                audio: None,
                video: Some(RawVideoOptions {
                    resolution: Resolution {
                        width: bench_config.output_resolution.width,
                        height: bench_config.output_resolution.height,
                    },
                }),
            },
        },
    )?;
    Ok(Box::new(x.video.unwrap()))
}

fn register_pipeline_mp4_input(
    pipeline: &Arc<Mutex<Pipeline>>,
    input_id: InputId,
    bench_config: &SingleBenchConfig,
) -> Result<InputInitInfo, RegisterInputError> {
    let video_decoder = bench_config.video_decoder;
    Pipeline::register_input(
        pipeline,
        input_id,
        RegisterInputOptions {
            input_options: InputOptions::Mp4(Mp4Options {
                should_loop: true,
                video_decoder,
                source: Source::File(bench_config.file_path.clone()),
            }),
            queue_options: QueueInputOptions {
                offset: Some(Duration::ZERO),
                required: true,
                buffer_duration: None,
            },
        },
    )
}

fn register_pipeline_raw_input(
    pipeline: &Arc<Mutex<Pipeline>>,
    input_id: InputId,
) -> Result<Sender<PipelineEvent<Frame>>, RegisterInputError> {
    let input = Pipeline::register_raw_data_input(
        pipeline,
        input_id,
        RawDataInputOptions {
            video: true,
            audio: false,
        },
        QueueInputOptions {
            offset: Some(Duration::ZERO),
            required: true,
            buffer_duration: None,
        },
    )?;

    Ok(input.video.unwrap())
}

fn raw_data_sender(bench_config: SingleBenchConfig, senders: Vec<Sender<PipelineEvent<Frame>>>) {
    let mut file = File::open(bench_config.file_path).unwrap();

    let args::Resolution { width, height } = bench_config.input_resolution.unwrap();
    let dimensions = width * height;
    let mut buffer = vec![0u8; dimensions * 3 / 2];

    let mut frame_nr = 0;
    while let Ok(()) = file.read_exact(&mut buffer) {
        if buffer.len() < dimensions * 3 / 2 {
            return;
        }
        let y_plane = &buffer[..dimensions];
        let u_plane = &buffer[dimensions..dimensions * 5 / 4];
        let v_plane = &buffer[dimensions * 5 / 4..];
        let frame = Frame {
            data: compositor_render::FrameData::PlanarYuv420(YuvPlanes {
                y_plane: bytes::Bytes::from(y_plane.to_vec()),
                u_plane: bytes::Bytes::from(u_plane.to_vec()),
                v_plane: bytes::Bytes::from(v_plane.to_vec()),
            }),
            resolution: Resolution { width, height },
            pts: Duration::from_secs((frame_nr / bench_config.input_framerate.unwrap()).into()),
        };
        for sender in &senders {
            let _ = sender.send(PipelineEvent::Data(frame.clone()));
        }
        frame_nr += 1;
    }
}
