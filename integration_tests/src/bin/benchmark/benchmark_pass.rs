use anyhow::{anyhow, Result};
use std::{
    fmt,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use compositor_pipeline::{
    codecs::*,
    error::{RegisterInputError, RegisterOutputError},
    graphics_context::GraphicsContext,
    protocols::*,
    *,
};
use compositor_render::{
    scene::Component, Frame, InputId, OutputId, RendererId, RendererSpec, RenderingMode, YuvPlanes,
};
use crossbeam_channel::{Receiver, Sender, TryRecvError};
use tracing::debug;

use crate::{
    args::Resolution,
    benchmark::EncoderOptions,
    scenes::{SceneBuilderFn, SceneContext},
    utils::benchmark_pipeline_options,
};

#[derive(Debug, Clone)]
pub enum InputFile {
    Mp4(PathBuf),
    Raw(RawInputFile),
}

#[derive(Clone)]
pub struct RawInputFile {
    pub frames: Arc<Vec<YuvPlanes>>,
    pub resolution: Resolution,
    pub framerate: f64,
}

impl fmt::Debug for RawInputFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RawInputFile").finish()
    }
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

impl DurationReceiver for Receiver<EncodedOutputEvent> {
    fn try_receive(&self) -> Result<Duration, TryRecvError> {
        loop {
            match self.try_recv() {
                Ok(EncodedOutputEvent::AudioEOS) => (),
                Ok(EncodedOutputEvent::VideoEOS) => (),
                Ok(EncodedOutputEvent::Data(chunk)) => match chunk.kind {
                    MediaKind::Video(_) => return Ok(chunk.pts),
                    MediaKind::Audio(_) => (),
                },
                Err(err) => return Err(err),
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct SingleBenchmarkPass {
    pub scene_builder: SceneBuilderFn,
    pub resources: Vec<(RendererId, RendererSpec)>,
    pub input_count: u64,
    pub output_count: u64,
    pub framerate: u64,
    pub input_file: InputFile,
    pub output_resolution: Resolution,
    pub encoder: EncoderOptions,
    pub warm_up_time: Duration,
    pub decoder: VideoDecoderOptions,
    pub rendering_mode: RenderingMode,
}

impl SingleBenchmarkPass {
    pub fn run(&self, ctx: GraphicsContext) -> Result<bool> {
        let (pipeline, _event_loop) = Pipeline::new(benchmark_pipeline_options(
            self.framerate,
            ctx,
            self.rendering_mode,
        ))?;

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
        for (id, spec) in self.resources.clone() {
            Pipeline::register_renderer(&pipeline, id, spec)?;
        }
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
            .map(|receiver| self.listen_on_output(start_time, receiver))
            .collect();

        debug!("waiting for results");
        let mut result = true;
        for output_thread in output_threads {
            if !output_thread.join().map_err(|_| anyhow!("thread panic"))? {
                result = false;
            }
        }
        Ok(result)
    }

    fn register_pipeline_encoded_output(
        &self,
        pipeline: &Arc<Mutex<Pipeline>>,
        output_id: &OutputId,
        root: Component,
        preset: FfmpegH264EncoderPreset,
    ) -> Result<Box<dyn DurationReceiver + Send>, RegisterOutputError> {
        let result = Pipeline::register_encoded_data_output(
            pipeline,
            output_id.clone(),
            RegisterEncodedDataOutputOptions {
                video: Some(RegisterOutputVideoOptions {
                    initial: root,
                    end_condition: PipelineOutputEndCondition::Never,
                }),
                audio: None,
                output_options: EncodedDataOutputOptions {
                    audio: None,
                    video: Some(VideoEncoderOptions::FfmpegH264(FfmpegH264EncoderOptions {
                        preset,
                        resolution: compositor_render::Resolution {
                            width: self.output_resolution.width,
                            height: self.output_resolution.height,
                        },
                        pixel_format: OutputPixelFormat::YUV420P,
                        raw_options: vec![("threads".to_string(), "0".to_string())],
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
            RegisterRawDataOutputOptions {
                video: Some(RegisterOutputVideoOptions {
                    initial: root,
                    end_condition: PipelineOutputEndCondition::Never,
                }),
                audio: None,
                output_options: RawDataOutputOptions {
                    audio: None,
                    video: Some(RawDataOutputVideoOptions {
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
                input_options: ProtocolInputOptions::Mp4(Mp4InputOptions {
                    should_loop: true,
                    video_decoders: Mp4InputVideoDecoders {
                        h264: Some(self.decoder),
                    },
                    source: Mp4InputSource::File(path.to_path_buf().into()),
                }),
                queue_options: QueueInputOptions {
                    offset: None,
                    required: true,
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
                buffer_duration: None,
            },
            QueueInputOptions {
                offset: None,
                required: true,
            },
        )?;

        Ok(input.video.unwrap())
    }

    fn listen_on_output(
        &self,
        start_time: Instant,
        receiver: Box<dyn DurationReceiver + Send>,
    ) -> thread::JoinHandle<bool> {
        let warm_up_time = self.warm_up_time;
        debug!("start listening for output frames");
        thread::spawn(move || {
            debug!("start drain in warm up mode");
            while start_time.elapsed() < warm_up_time {
                if let Err(TryRecvError::Empty) = receiver.try_receive() {
                    thread::sleep(Duration::from_millis(10));
                }
            }

            const FIRST_CHECK: Duration = Duration::from_secs(6);
            const SECOND_CHECK: Duration = Duration::from_secs(12);
            const LAST_CHECK: Duration = Duration::from_secs(30);

            debug!("start drain in measure mode");
            let mut max_pts: Duration = Duration::ZERO;
            let mut min_pts: Duration = Duration::MAX;
            let receive_fn = move |min_pts: &mut Duration, max_pts: &mut Duration| match receiver
                .try_receive()
            {
                Err(TryRecvError::Empty) => {
                    thread::sleep(Duration::from_millis(10));
                }
                Err(TryRecvError::Disconnected) => panic!(),
                Ok(pts) => {
                    *max_pts = (*max_pts).max(pts);
                    *min_pts = (*min_pts).min(pts);
                }
            };

            // First check after 6 second
            while start_time.elapsed() < warm_up_time + FIRST_CHECK {
                receive_fn(&mut min_pts, &mut max_pts)
            }
            let measured_time = max_pts.saturating_sub(min_pts);
            if measured_time < FIRST_CHECK - Duration::from_millis(1200) {
                debug!("FAIL first check - {:?}", measured_time);
                return false;
            } else if measured_time > FIRST_CHECK - Duration::from_millis(50) {
                debug!("PASS first check - {:?}", measured_time);
                return true;
            } else {
                debug!("continue check - {:?}", measured_time);
            }

            // Second check after 12 second
            while start_time.elapsed() < warm_up_time + SECOND_CHECK {
                receive_fn(&mut min_pts, &mut max_pts)
            }
            let measured_time = max_pts.saturating_sub(min_pts);
            if measured_time < SECOND_CHECK - Duration::from_millis(800) {
                debug!("FAIL second check - {:?}", measured_time);
                return false;
            } else if measured_time > SECOND_CHECK - Duration::from_millis(100) {
                debug!("PASS second check - {:?}", measured_time);
                return true;
            } else {
                debug!("continue check - {:?}", measured_time);
            }

            // Last check
            while start_time.elapsed() < warm_up_time + LAST_CHECK {
                receive_fn(&mut min_pts, &mut max_pts)
            }
            let measured_time = max_pts.saturating_sub(min_pts);
            if measured_time > LAST_CHECK - Duration::from_millis(800) {
                debug!("PASS last check - {:?}", measured_time);
                true
            } else {
                debug!("FAIL last check - {:?}", measured_time);
                false
            }
        })
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
