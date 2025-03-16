use core::panic;
use std::{
    fs::File,
    io::Write,
    path::PathBuf,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use compositor_pipeline::{
    audio_mixer::{AudioChannels, AudioMixingParams, AudioSamples, InputParams, MixingStrategy},
    pipeline::{
        input::{
            mp4::{Mp4Options, Source},
            InputOptions,
        },
        output::{RawAudioOptions, RawDataOutputOptions, RawVideoOptions},
        GraphicsContext, Options, PipelineOutputEndCondition, RawDataReceiver,
        RegisterInputOptions, RegisterOutputOptions,
    },
    queue::{PipelineEvent, QueueInputOptions},
    Pipeline,
};
use compositor_render::{
    error::ErrorStack,
    scene::{Component, InputStreamComponent},
    Frame, FrameData, InputId, OutputId, RenderingMode, Resolution,
};
use crossbeam_channel::bounded;
use image::{codecs::png::PngEncoder, ColorType, ImageEncoder};
use integration_tests::{examples::download_file, read_rgba_texture};
use smelter::{
    config::read_config,
    logger::{self},
};
use tokio::runtime::Runtime;

const BUNNY_FILE_URL: &str =
    "https://commondatastorage.googleapis.com/gtv-videos-bucket/sample/BigBuckBunny.mp4";
const BUNNY_FILE_PATH: &str = "examples/assets/BigBuckBunny.mp4";

fn root_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

// Start simple pipeline with output that sends PCM audio and wgpu::Textures via Rust channel.
//
// Output:
// - read content of wgpu texture and write it as png file
// - read audio samples and write raw value using debug formatting
fn main() {
    ffmpeg_next::format::network::init();
    logger::init_logger(read_config().logger);
    let mut config = read_config();
    config.queue_options.ahead_of_time_processing = true;
    let ctx = GraphicsContext::new(Default::default()).unwrap();
    let (wgpu_device, wgpu_queue) = (ctx.device.clone(), ctx.queue.clone());
    // no chromium support, so we can ignore _event_loop
    let (pipeline, _event_loop) = Pipeline::new(Options {
        queue_options: config.queue_options,
        stream_fallback_timeout: config.stream_fallback_timeout,
        web_renderer: config.web_renderer,
        force_gpu: config.force_gpu,
        download_root: config.download_root,
        mixing_sample_rate: config.mixing_sample_rate,
        stun_servers: config.stun_servers,
        wgpu_features: config.required_wgpu_features,
        load_system_fonts: Some(true),
        wgpu_ctx: Some(ctx),
        whip_whep_server_port: Some(config.whip_whep_server_port),
        start_whip_whep: config.start_whip_whep,
        tokio_rt: Some(Arc::new(Runtime::new().unwrap())),
        rendering_mode: RenderingMode::GpuOptimized,
    })
    .unwrap_or_else(|err| {
        panic!(
            "Failed to start compositor.\n{}",
            ErrorStack::new(&err).into_string()
        )
    });
    let pipeline = Arc::new(Mutex::new(pipeline));
    let output_id = OutputId("output_1".into());
    let input_id = InputId("input_id".into());

    download_file(BUNNY_FILE_URL, BUNNY_FILE_PATH).unwrap();

    let output_options = RegisterOutputOptions {
        output_options: RawDataOutputOptions {
            video: Some(RawVideoOptions {
                resolution: Resolution {
                    width: 1280,
                    height: 720,
                },
            }),
            audio: Some(RawAudioOptions),
        },
        video: Some(compositor_pipeline::pipeline::OutputVideoOptions {
            initial: Component::InputStream(InputStreamComponent {
                id: None,
                input_id: input_id.clone(),
            }),
            end_condition: PipelineOutputEndCondition::Never,
        }),
        audio: Some(compositor_pipeline::pipeline::OutputAudioOptions {
            initial: AudioMixingParams {
                inputs: vec![InputParams {
                    input_id: input_id.clone(),
                    volume: 1.0,
                }],
            },
            mixing_strategy: MixingStrategy::SumClip,
            channels: AudioChannels::Stereo,
            end_condition: PipelineOutputEndCondition::Never,
        }),
    };

    let input_options = RegisterInputOptions {
        input_options: InputOptions::Mp4(Mp4Options {
            source: Source::File(root_dir().join(BUNNY_FILE_PATH)),
            should_loop: false,
            video_decoder: compositor_pipeline::pipeline::VideoDecoder::FFmpegH264,
        }),
        queue_options: QueueInputOptions {
            required: true,
            offset: Some(Duration::ZERO),
            buffer_duration: None,
        },
    };

    Pipeline::register_input(&pipeline, input_id.clone(), input_options).unwrap();

    let RawDataReceiver { video, audio } =
        Pipeline::register_raw_data_output(&pipeline, output_id.clone(), output_options).unwrap();

    Pipeline::start(&pipeline);

    let (send_done, recv_done) = bounded(0);

    thread::Builder::new()
        .spawn(move || {
            for (index, frame) in video.unwrap().iter().enumerate() {
                if [0, 200, 400, 600, 800, 1000].contains(&index) {
                    write_frame(index, frame, &wgpu_device, &wgpu_queue);
                }
                if index > 1000 {
                    send_done.send(()).unwrap();
                    return;
                }
            }
        })
        .unwrap();

    let mut audio_dump =
        File::create(root_dir().join("examples/raw_channel_output_audio_dump.debug")).unwrap();

    thread::Builder::new()
        .spawn(move || {
            for packet in audio.unwrap().iter() {
                if let PipelineEvent::Data(packet) = packet {
                    let AudioSamples::Stereo(samples) = packet.samples else {
                        continue;
                    };
                    audio_dump
                        .write_all(format!("{:?} {:?}\n", packet.start_pts, samples).as_bytes())
                        .unwrap();
                } else {
                    return;
                };
            }
        })
        .unwrap();

    recv_done.recv().unwrap()
}

fn write_frame(
    index: usize,
    frame: PipelineEvent<Frame>,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) {
    let PipelineEvent::Data(frame) = frame else {
        return;
    };
    let FrameData::Rgba8UnormWgpuTexture(texture) = frame.data else {
        return;
    };
    let size = texture.size();
    let frame_data = read_rgba_texture(device, queue, &texture);

    let filepath = root_dir().join(format!(
        "examples/raw_channel_output_video_frame_{}.png",
        index
    ));
    let file = File::create(filepath).unwrap();
    let encoder = PngEncoder::new(file);
    encoder
        .write_image(&frame_data, size.width, size.height, ColorType::Rgba8)
        .unwrap();
}
