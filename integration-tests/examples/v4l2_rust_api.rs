#[cfg(not(target_os = "linux"))]
fn main() {
    panic!("Your OS does not support Video for Linux 2.");
}

#[cfg(target_os = "linux")]
fn main() {
    main_module::run();
}

#[cfg(target_os = "linux")]
mod main_module {
    use std::{
        sync::{Arc, Mutex},
        time::Duration,
    };

    use integration_tests::ffmpeg::start_ffmpeg_rtmp_receive;
    use smelter::{
        config::{Config, read_config},
        logger,
        state::pipeline_options_from_config,
    };
    use smelter_core::{codecs::*, protocols::*, *};
    use smelter_render::{
        Framerate, InputId, OutputId, Resolution,
        error::ErrorStack,
        scene::{Component, InputStreamComponent},
    };
    use tokio::runtime::Runtime;

    const VIDEO_RESOLUTION: Resolution = Resolution {
        width: 1920,
        height: 1080,
    };

    const PORT: u16 = 8010;

    pub fn run() {
        ffmpeg_next::format::network::init();
        let config = read_config();
        logger::init_logger(config.logger.clone());

        #[allow(clippy::zombie_processes)]
        start_ffmpeg_rtmp_receive(PORT).unwrap();

        let pipeline = pipeline(&config);
        let output_id = OutputId("output_1".into());
        let input_id = InputId("input_1".into());

        Pipeline::register_input(&pipeline, input_id.clone(), v4l2_input_options()).unwrap();
        Pipeline::register_output(&pipeline, output_id, output_options(input_id.clone())).unwrap();
        Pipeline::start(&pipeline);

        std::thread::sleep(Duration::from_secs(30));
    }

    fn v4l2_input_options() -> RegisterInputOptions {
        let devices = list_v4l2_devices("/dev/v4l/by-id/").unwrap();

        let device = devices
            .into_iter()
            .find(|d| d.formats.iter().any(|f| f.format == V4l2Format::Yuyv))
            .expect("no device supports the required format");

        RegisterInputOptions {
            input_options: ProtocolInputOptions::V4l2(V4l2InputOptions {
                path: device.path,
                resolution: Some(VIDEO_RESOLUTION),
                format: V4l2Format::Yuyv,
                framerate: Some(Framerate { num: 30, den: 1 }),
            }),
            queue_options: QueueInputOptions {
                required: false,
                offset: None,
            },
        }
    }

    #[cfg(target_os = "linux")]
    fn pipeline(config: &Config) -> Arc<Mutex<Pipeline>> {
        Arc::new(Mutex::new(
            Pipeline::new(PipelineOptions {
                wgpu_options: PipelineWgpuOptions::Options {
                    device_id: None,
                    driver_name: None,
                    features: wgpu::Features::empty(),
                    force_gpu: false,
                },
                ..pipeline_options_from_config(config, &Arc::new(Runtime::new().unwrap()), &None)
            })
            .unwrap_or_else(|err| {
                panic!(
                    "Failed to start compositor.\n{}",
                    ErrorStack::new(&err).into_string()
                )
            }),
        ))
    }

    fn output_options(input_id: InputId) -> RegisterOutputOptions {
        RegisterOutputOptions {
            output_options: ProtocolOutputOptions::Rtmp(RtmpOutputOptions {
                video: Some(VideoEncoderOptions::FfmpegH264(FfmpegH264EncoderOptions {
                    preset: FfmpegH264EncoderPreset::Ultrafast,
                    resolution: VIDEO_RESOLUTION,
                    pixel_format: OutputPixelFormat::YUV420P,
                    raw_options: vec![
                        ("tune".into(), "zerolatency".into()),
                        ("thread_type".into(), "slice".into()),
                    ],
                })),
                audio: None,
                url: format!("rtmp://127.0.0.1:{PORT}").into(),
            }),
            video: Some(RegisterOutputVideoOptions {
                initial: Component::InputStream(InputStreamComponent { id: None, input_id }),
                end_condition: PipelineOutputEndCondition::Never,
            }),
            audio: None, // TODO: add audio example
        }
    }
}
