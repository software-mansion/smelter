use core::panic;
use std::{fs::File, io::Write, path::PathBuf, sync::Arc, time::Duration};

use compositor_pipeline::{
    audio_mixer::{AudioChannels, AudioMixingParams, InputParams, MixingStrategy},
    pipeline::{
        encoder::{
            self, ffmpeg_h264, AudioEncoderOptions, AudioEncoderPreset, VideoEncoderOptions,
        },
        input::{
            mp4::{Mp4Options, Source},
            InputOptions,
        },
        output::EncodedDataOutputOptions,
        AudioCodec, EncodedChunkKind, EncoderOutputEvent, Pipeline, PipelineOutputEndCondition,
        RegisterInputOptions, RegisterOutputOptions, VideoCodec,
    },
    queue::QueueInputOptions,
};
use compositor_render::{
    error::ErrorStack,
    scene::{Component, InputStreamComponent},
    InputId, OutputId, Resolution,
};
use integration_tests::examples::download_file;
use smelter::{config::read_config, logger, state::ApiState};
use tokio::runtime::Runtime;

const BUNNY_FILE_URL: &str =
    "https://commondatastorage.googleapis.com/gtv-videos-bucket/sample/BigBuckBunny.mp4";
const BUNNY_FILE_PATH: &str = "examples/assets/BigBuckBunny.mp4";

// Start simple pipeline with output that sends encoded video/audio via Rust channel.
//
// Data read from channels are dumped into files as it is without any timestamp data.
fn main() {
    ffmpeg_next::format::network::init();
    let mut config = read_config();
    logger::init_logger(config.logger.clone());
    let root_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    config.queue_options.ahead_of_time_processing = true;
    // no chromium support, so we can ignore _event_loop
    let runtime = Arc::new(Runtime::new().unwrap());
    let (state, _event_loop) = ApiState::new(config, runtime).unwrap_or_else(|err| {
        panic!(
            "Failed to start compositor.\n{}",
            ErrorStack::new(&err).into_string()
        )
    });
    let output_id = OutputId("output_1".into());
    let input_id = InputId("input_id".into());

    download_file(BUNNY_FILE_URL, BUNNY_FILE_PATH).unwrap();

    let output_options = RegisterOutputOptions {
        output_options: EncodedDataOutputOptions {
            video: Some(VideoEncoderOptions::H264(ffmpeg_h264::Options {
                preset: ffmpeg_h264::EncoderPreset::Ultrafast,
                resolution: Resolution {
                    width: 1280,
                    height: 720,
                },
                raw_options: vec![],
            })),
            audio: Some(AudioEncoderOptions::Opus(
                encoder::opus::OpusEncoderOptions {
                    channels: AudioChannels::Stereo,
                    preset: AudioEncoderPreset::Voip,
                    sample_rate: 48000,
                },
            )),
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
            source: Source::File(root_dir.join(BUNNY_FILE_PATH)),
            should_loop: false,
            video_decoder: compositor_pipeline::pipeline::VideoDecoder::FFmpegH264,
        }),
        queue_options: QueueInputOptions {
            required: true,
            offset: Some(Duration::ZERO),
            buffer_duration: None,
        },
    };

    Pipeline::register_input(&state.pipeline, input_id.clone(), input_options).unwrap();

    let output_receiver =
        Pipeline::register_encoded_data_output(&state.pipeline, output_id.clone(), output_options)
            .unwrap();

    Pipeline::start(&state.pipeline);

    let mut h264_dump =
        File::create(root_dir.join("examples/encoded_channel_output_dump.h264")).unwrap();
    let mut opus_dump =
        File::create(root_dir.join("examples/encoded_channel_output_dump.opus")).unwrap();

    for (index, chunk) in output_receiver.iter().enumerate() {
        if index > 3000 {
            return;
        }
        let EncoderOutputEvent::Data(chunk) = chunk else {
            return;
        };
        match chunk.kind {
            EncodedChunkKind::Video(VideoCodec::H264) => h264_dump.write_all(&chunk.data).unwrap(),
            EncodedChunkKind::Video(VideoCodec::VP8) => unimplemented!(),
            EncodedChunkKind::Audio(AudioCodec::Opus) => opus_dump.write_all(&chunk.data).unwrap(),
            EncodedChunkKind::Audio(AudioCodec::Aac) => panic!("AAC is not supported on output"),
        }
    }
}
