use anyhow::{anyhow, Result};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use compositor_pipeline::{
    error::{RegisterInputError, RegisterOutputError},
    pipeline::{
        self,
        encoder::{
            ffmpeg_h264::{self, EncoderPreset},
            VideoEncoderOptions,
        },
        input::{
            mp4::{Mp4Options, Source},
            InputInitInfo, InputOptions, RawDataInputOptions,
        },
        output::{EncodedDataOutputOptions, RawDataOutputOptions, RawVideoOptions},
        EncodedChunkKind, EncoderOutputEvent, GraphicsContext, OutputVideoOptions,
        PipelineOutputEndCondition, RegisterInputOptions, RegisterOutputOptions,
    },
    queue::{PipelineEvent, QueueInputOptions},
    Pipeline,
};
use compositor_render::{scene::Component, Frame, InputId, OutputId, YuvPlanes};
use crossbeam_channel::{Receiver, Sender, TryRecvError};
use tracing::debug;

use crate::{
    args::Resolution, benchmark::EncoderOptions, scenes::SceneContext,
    utils::benchmark_pipeline_options,
};

#[derive(Debug, Clone)]
pub enum InputFile {
    Mp4(PathBuf),
    Raw(RawInputFile),
}

#[derive(Debug, Clone)]
pub struct RawInputFile {
    pub frames: Arc<Vec<YuvPlanes>>,
    pub resolution: Resolution,
    pub framerate: f64,
}

trait DurationReceiver {
    fn try_receive(&self) -> Result<Duration, TryRecvError>;
}

impl DurationReceiver for Receiver<PipelineEvent<Frame>> {
    fn try_receive(&self) -> Result<Duration, TryRecvError> {
        loop {
            match self.try_recv() {
                Ok(PipelineEvent::EOS) => (),
                Ok(PipelineEvent::Data(frame)) => return Ok(frame.pts),
                Err(err) => return Err(err),
            }
        }
    }
}

impl DurationReceiver for Receiver<EncoderOutputEvent> {
    fn try_receive(&self) -> Result<Duration, TryRecvError> {
        loop {
            match self.try_recv() {
                Ok(EncoderOutputEvent::AudioEOS) => (),
                Ok(EncoderOutputEvent::VideoEOS) => (),
                Ok(EncoderOutputEvent::Data(chunk)) => match chunk.kind {
                    EncodedChunkKind::Video(_) => return Ok(chunk.pts),
                    EncodedChunkKind::Audio(_) => (),
                },
                Err(err) => return Err(err),
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct SingleBenchmarkPass {
    pub scene_builder: fn(ctx: &SceneContext, output_id: &OutputId) -> Component,
    pub input_count: u64,
    pub output_count: u64,
    pub framerate: u64,
    pub input_file: InputFile,
    pub output_resolution: Resolution,
    pub encoder: EncoderOptions,
    pub warm_up_time: Duration,
    pub measure_time: Duration,
    pub decoder: pipeline::VideoDecoder,
    pub error_tolerance_multiplier: f64,
}

impl SingleBenchmarkPass {
    pub fn run(&self, ctx: GraphicsContext) -> Result<bool> {
        let (pipeline, _event_loop) = Pipeline::new(pipeline::Options {
            wgpu_ctx: Some(ctx),
            ..benchmark_pipeline_options(self.framerate)
        })?;

        let pipeline = Arc::new(Mutex::new(pipeline));

        debug!("pipeline start");
        Pipeline::start(&pipeline);

        let inputs: Vec<_> = (0..self.input_count)
            .map(|i| InputId(format!("input_{i}").into()))
            .collect();
        let outputs: Vec<_> = (0..self.output_count)
            .map(|i| OutputId(format!("output_{i}").into()))
            .collect();

        match self.input_file.clone() {
            InputFile::Raw(input) => {
                let frame_senders = inputs
                    .iter()
                    .cloned()
                    .map(|input_id| Ok(self.register_pipeline_raw_input(&pipeline, input_id)?))
                    .collect::<Result<Vec<_>>>()?;
                thread::spawn(move || raw_data_sender(frame_senders, input));
            }
            InputFile::Mp4(path) => {
                for input_id in &inputs {
                    self.register_pipeline_mp4_input(&pipeline, input_id, &path)?;
                }
            }
        };

        let scene_ctx = SceneContext {
            inputs: inputs.clone(),
            outputs: outputs.clone(),
        };
        let receivers = outputs
            .iter()
            .map(|output_id| {
                let root = (self.scene_builder)(&scene_ctx, output_id);
                let receiver: Box<dyn DurationReceiver + Send> = match self.encoder {
                    EncoderOptions::Disabled => {
                        self.register_pipeline_raw_output(&pipeline, output_id, root)?
                    }
                    EncoderOptions::Enabled(preset) => {
                        self.register_pipeline_encoded_output(&pipeline, output_id, root, preset)?
                    }
                };

                Ok(receiver)
            })
            .collect::<Result<Vec<_>>>()?;

        let start_time = Instant::now();
        let output_threads: Vec<_> = receivers
            .into_iter()
            .enumerate()
            .map(|(index, receiver)| {
                let warm_up_time = self.warm_up_time;
                let measure_time = self.measure_time;
                let error_tolerance_multiplier = self.error_tolerance_multiplier;
                debug!("start listening for output frames");
                thread::spawn(move || {
                    debug!(?index, "start drain in warm up mode");
                    while start_time.elapsed() < warm_up_time {
                        if let Err(TryRecvError::Empty) = receiver.try_receive() {
                            thread::sleep(Duration::from_millis(1));
                        }
                    }

                    debug!(?index, "start drain in measure mode");
                    let mut max_pts: Duration = Duration::ZERO;
                    let mut min_pts: Duration = Duration::MAX;
                    while start_time.elapsed() < measure_time + warm_up_time {
                        match receiver.try_receive() {
                            Err(TryRecvError::Empty) => {
                                thread::sleep(Duration::from_millis(1));
                            }
                            Err(TryRecvError::Disconnected) => panic!(),
                            Ok(pts) => {
                                max_pts = max_pts.max(pts);
                                min_pts = min_pts.min(pts);
                            }
                        }
                    }

                    let expected_duration_sec =
                        measure_time.as_secs_f64() / error_tolerance_multiplier;

                    // true - processing on time
                    // false - processing falling behind
                    (max_pts - min_pts).as_secs_f64() > expected_duration_sec
                })
            })
            .collect();

        debug!("waiting for results");
        for output_thread in output_threads {
            if !output_thread.join().map_err(|_| anyhow!("thread panic"))? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn register_pipeline_encoded_output(
        &self,
        pipeline: &Arc<Mutex<Pipeline>>,
        output_id: &OutputId,
        root: Component,
        preset: EncoderPreset,
    ) -> Result<Box<dyn DurationReceiver + Send>, RegisterOutputError> {
        let result = Pipeline::register_encoded_data_output(
            pipeline,
            output_id.clone(),
            RegisterOutputOptions {
                video: Some(OutputVideoOptions {
                    initial: root,
                    end_condition: PipelineOutputEndCondition::Never,
                }),
                audio: None,
                output_options: EncodedDataOutputOptions {
                    audio: None,
                    video: Some(VideoEncoderOptions::H264(ffmpeg_h264::Options {
                        preset,
                        resolution: compositor_render::Resolution {
                            width: self.output_resolution.width,
                            height: self.output_resolution.height,
                        },
                        raw_options: Vec::new(),
                    })),
                },
            },
        )?;
        Ok(Box::new(result))
    }

    fn register_pipeline_raw_output(
        &self,
        pipeline: &Arc<Mutex<Pipeline>>,
        output_id: &OutputId,
        root: Component,
    ) -> Result<Box<dyn DurationReceiver + Send>, RegisterOutputError> {
        let result = Pipeline::register_raw_data_output(
            pipeline,
            output_id.clone(),
            RegisterOutputOptions {
                video: Some(OutputVideoOptions {
                    initial: root,
                    end_condition: PipelineOutputEndCondition::Never,
                }),
                audio: None,
                output_options: RawDataOutputOptions {
                    audio: None,
                    video: Some(RawVideoOptions {
                        resolution: compositor_render::Resolution {
                            width: self.output_resolution.width,
                            height: self.output_resolution.height,
                        },
                    }),
                },
            },
        )?;
        Ok(Box::new(result.video.unwrap()))
    }

    fn register_pipeline_mp4_input(
        &self,
        pipeline: &Arc<Mutex<Pipeline>>,
        input_id: &InputId,
        path: &Path,
    ) -> Result<InputInitInfo, RegisterInputError> {
        Pipeline::register_input(
            pipeline,
            input_id.clone(),
            RegisterInputOptions {
                input_options: InputOptions::Mp4(Mp4Options {
                    should_loop: true,
                    video_decoder: self.decoder,
                    source: Source::File(path.to_path_buf()),
                }),
                queue_options: QueueInputOptions {
                    offset: None,
                    required: true,
                    buffer_duration: None,
                },
            },
        )
    }

    fn register_pipeline_raw_input(
        &self,
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
                offset: None,
                required: true,
                buffer_duration: None,
            },
        )?;

        Ok(input.video.unwrap())
    }
}

fn raw_data_sender(senders: Vec<Sender<PipelineEvent<Frame>>>, input: RawInputFile) {
    if senders.is_empty() {
        return;
    }

    let mut counter: u64 = 0;
    loop {
        for yuv_planes in input.frames.iter() {
            let frame = Frame {
                data: compositor_render::FrameData::PlanarYuv420(yuv_planes.clone()),
                resolution: compositor_render::Resolution {
                    width: input.resolution.width,
                    height: input.resolution.height,
                },
                pts: Duration::from_secs_f64(counter as f64 / input.framerate),
            };
            counter += 1;

            for sender in &senders {
                if sender.send(PipelineEvent::Data(frame.clone())).is_err() {
                    debug!("Stopping raw data sender");
                    return;
                }
            }
        }
    }
}
