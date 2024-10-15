use anyhow::Result;
use compositor_pipeline::pipeline::output::whip::WhipSenderOptions;
use integration_tests::examples::download_all_assets;
use live_compositor::{
    config::{LoggerConfig, LoggerFormat},
    logger::{self, FfmpegLogLevel},
};

fn main() {
    ffmpeg_next::format::network::init();
    logger::init_logger(LoggerConfig {
        ffmpeg_logger_level: FfmpegLogLevel::Info,
        format: LoggerFormat::Compact,
        level: "info,wgpu_hal=warn,wgpu_core=warn".to_string(),
    });

    download_all_assets().unwrap();

    client_code().unwrap();
}

fn client_code() -> Result<()> {
    use compositor_api::types::Resolution;
    use compositor_pipeline::{
        pipeline::{
            decoder::VideoDecoderOptions,
            encoder::{
                ffmpeg_h264::{EncoderPreset, Options as H264Options},
                VideoEncoderOptions,
            },
            input::{
                rtp::{InputVideoStream, RtpReceiverOptions, RtpStream},
                InputOptions,
            },
            output::{OutputOptions, OutputProtocolOptions},
            rtp::{RequestedPort, TransportProtocol},
            Options, OutputVideoOptions, PipelineOutputEndCondition, Port, RegisterInputOptions,
            RegisterOutputOptions, VideoCodec, VideoDecoder,
        },
        queue::QueueInputOptions,
        Pipeline,
    };
    use compositor_render::{
        error::ErrorStack,
        scene::{
            Component, ComponentId, HorizontalAlign, InputStreamComponent, RGBAColor,
            TilesComponent, VerticalAlign,
        },
        InputId, OutputId,
    };
    use live_compositor::config::read_config;
    use signal_hook::{consts, iterator::Signals};
    use std::{
        sync::{Arc, Mutex},
        time::Duration,
    };

    use integration_tests::{
        examples::TestSample,
        ffmpeg::{start_ffmpeg_receive, start_ffmpeg_send},
    };

    const VIDEO_RESOLUTION: Resolution = Resolution {
        width: 1280,
        height: 720,
    };

    const IP: &str = "127.0.0.1";
    const INPUT_PORT: u16 = 8002;
    const OUTPUT_PORT: u16 = 8004;

    const VIDEOS: u16 = 6;
    start_ffmpeg_receive(Some(OUTPUT_PORT), None)?;

    let config = read_config();
    let (pipeline, event_loop) = Pipeline::new(Options {
        queue_options: config.queue_options,
        stream_fallback_timeout: config.stream_fallback_timeout,
        web_renderer: config.web_renderer,
        force_gpu: config.force_gpu,
        download_root: config.download_root,
        output_sample_rate: config.output_sample_rate,
        wgpu_features: config.required_wgpu_features,
        load_system_fonts: None,
        wgpu_ctx: None,
    })
    .unwrap_or_else(|err| {
        panic!(
            "Failed to start compositor.\n{}",
            ErrorStack::new(&err).into_string()
        )
    });

    let pipeline = Arc::new(Mutex::new(pipeline));

    let input_id = InputId(format!("input_1").into());

    let input_options = RegisterInputOptions {
        input_options: InputOptions::Rtp(RtpReceiverOptions {
            port: RequestedPort::Exact(INPUT_PORT),
            transport_protocol: TransportProtocol::Udp,
            stream: RtpStream {
                video: Some(InputVideoStream {
                    options: VideoDecoderOptions {
                        decoder: VideoDecoder::FFmpegH264,
                    },
                }),
                audio: None,
            },
        }),
        queue_options: QueueInputOptions {
            offset: Some(Duration::ZERO),
            required: false,
            buffer_duration: None,
        },
    };

    Pipeline::register_input(&pipeline, input_id.clone(), input_options).unwrap();

    // children.push(Component::InputStream(InputStreamComponent {
    //     id: None,
    //     input_id,
    // }));

    let output_options = RegisterOutputOptions {
        output_options: OutputOptions {
            output_protocol: OutputProtocolOptions::Whip(WhipSenderOptions {
                endpoint_url: "http://127.0.0.1:9000/whip".into(),
                video: Some(VideoCodec::H264),
                audio: None,
            }),
            video: Some(VideoEncoderOptions::H264(H264Options {
                preset: EncoderPreset::Ultrafast,
                resolution: VIDEO_RESOLUTION.into(),
                raw_options: Vec::new(),
            })),
            audio: None,
        },
        video: Some(OutputVideoOptions {
            initial: Component::Tiles(TilesComponent {
                id: Some(ComponentId("tiles".into())),
                padding: 5.0,
                background_color: RGBAColor(0x44, 0x44, 0x44, 0xff),
                children: Vec::new(),
                width: None,
                height: None,
                margin: 0.0,
                transition: None,
                vertical_align: VerticalAlign::Center,
                horizontal_align: HorizontalAlign::Center,
                tile_aspect_ratio: (16, 9),
            }),

            end_condition: PipelineOutputEndCondition::Never,
        }),
        audio: None,
    };

    pipeline
        .lock()
        .unwrap()
        .register_output(OutputId("output_1".into()), output_options)
        .unwrap();

    Pipeline::start(&pipeline);

    start_ffmpeg_send(IP, Some(INPUT_PORT), None, TestSample::BigBuckBunny)?;

    let event_loop_fallback = || {
        let mut signals = Signals::new([consts::SIGINT]).unwrap();
        signals.forever().next();
    };
    if let Err(err) = event_loop.run_with_fallback(&event_loop_fallback) {
        panic!(
            "Failed to start event loop.\n{}",
            ErrorStack::new(&err).into_string()
        )
    }

    Ok(())
}
