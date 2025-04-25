use std::{path::PathBuf, time::Duration};

use compositor_pipeline::pipeline::{
    self,
    encoder::ffmpeg_h264::{self, EncoderPreset},
    GraphicsContext, VideoDecoder,
};
use compositor_render::{scene::Component, OutputId};
use serde_json::json;
use tracing::{error, info};

use crate::{
    args::{NumericArgument, Resolution, ResolutionArgument},
    benchmark_pass::{InputFile, SingleBenchmarkPass},
    maximize_iter::{Const, MaximizeIter, MaximizeResolution, MaximizeU64},
    scenes::{simple_tiles_with_all_inputs, SceneContext},
};

#[derive(Debug, Clone, Copy)]
pub enum ValueOrMaximized<T: Clone + Copy> {
    Value(T),
    Maximize,
}

impl From<NumericArgument> for ValueOrMaximized<u64> {
    fn from(value: NumericArgument) -> Self {
        match value {
            NumericArgument::IterateExp => panic!(),
            NumericArgument::Maximize => ValueOrMaximized::Maximize,
            NumericArgument::Constant(v) => ValueOrMaximized::Value(v),
        }
    }
}

impl From<ResolutionArgument> for ValueOrMaximized<Resolution> {
    fn from(value: ResolutionArgument) -> Self {
        match value {
            ResolutionArgument::Iterate => panic!(),
            ResolutionArgument::Maximize => ValueOrMaximized::Maximize,
            ResolutionArgument::Constant(v) => ValueOrMaximized::Value(v),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum EncoderOptions {
    Enabled(ffmpeg_h264::EncoderPreset),
    Disabled,
}

#[derive(Debug, Clone)]
pub struct Benchmark {
    pub id: &'static str,
    pub scene_builder: fn(ctx: &SceneContext, output_id: &OutputId) -> Component,

    pub input_count: ValueOrMaximized<u64>,
    pub output_count: ValueOrMaximized<u64>,
    pub framerate: ValueOrMaximized<u64>,
    pub output_resolution: ValueOrMaximized<Resolution>,

    pub input_file: InputFile,
    pub encoder: EncoderOptions,
    pub decoder: pipeline::VideoDecoder,

    pub warm_up_time: Duration,
    pub measure_time: Duration,

    pub error_tolerance_multiplier: f64,
}

#[derive(Debug)]
pub struct BenchmarkResult {
    pub pass: Option<SingleBenchmarkPass>,
    pub config: Benchmark,
}

type Maximizer<T> = Box<dyn MaximizeIter<T>>;

impl Benchmark {
    pub fn run(&self, ctx: &GraphicsContext) -> BenchmarkResult {
        let (mut input_count, mut output_count, mut framerate, mut output_resolution) =
            self.maximizers();

        let mut last_valid_pass = None;
        let mut prev_success = true;
        while let (
            Some(input_count),
            Some(output_count),
            Some(framerate),
            Some(output_resolution),
        ) = (
            input_count.next(prev_success),
            output_count.next(prev_success),
            framerate.next(prev_success),
            output_resolution.next(prev_success),
        ) {
            let pass = SingleBenchmarkPass {
                input_count,
                output_count,
                framerate,
                output_resolution,
                ..self.default_single_bench()
            };
            prev_success = match pass.run(ctx.clone()) {
                Ok(is_success) => is_success,
                Err(err) => {
                    error!("Benchmark pass failed: {:#}", err);
                    false
                }
            };
            let status_str = match prev_success {
                true => "PASS",
                false => "FAIL",
            };
            info!(
                "{}: (input: {}, output: {}, framerate: {}, resolution: {:?})",
                status_str, input_count, output_count, framerate, output_resolution
            );
            if prev_success {
                last_valid_pass = Some(pass);
            }
        }

        BenchmarkResult {
            pass: last_valid_pass,
            config: self.clone(),
        }
    }

    fn maximizers(
        &self,
    ) -> (
        Maximizer<u64>,
        Maximizer<u64>,
        Maximizer<u64>,
        Maximizer<Resolution>,
    ) {
        match (
            self.input_count,
            self.output_count,
            self.framerate,
            self.output_resolution,
        ) {
            (
                ValueOrMaximized::Maximize,
                ValueOrMaximized::Value(output_count),
                ValueOrMaximized::Value(framerate),
                ValueOrMaximized::Value(output_resolution),
            ) => (
                Box::from(MaximizeU64::new(0)),
                Box::from(Const(output_count)),
                Box::from(Const(framerate)),
                Box::from(Const(output_resolution)),
            ),
            (
                ValueOrMaximized::Value(input_count),
                ValueOrMaximized::Maximize,
                ValueOrMaximized::Value(framerate),
                ValueOrMaximized::Value(output_resolution),
            ) => (
                Box::from(Const(input_count)),
                Box::from(MaximizeU64::new(0)),
                Box::from(Const(framerate)),
                Box::from(Const(output_resolution)),
            ),
            (
                ValueOrMaximized::Value(input_count),
                ValueOrMaximized::Value(output_count),
                ValueOrMaximized::Maximize,
                ValueOrMaximized::Value(output_resolution),
            ) => (
                Box::from(Const(input_count)),
                Box::from(Const(output_count)),
                Box::from(MaximizeU64::new(1)),
                Box::from(Const(output_resolution)),
            ),
            (
                ValueOrMaximized::Value(input_count),
                ValueOrMaximized::Value(output_count),
                ValueOrMaximized::Value(framerate),
                ValueOrMaximized::Maximize,
            ) => (
                Box::from(Const(input_count)),
                Box::from(Const(output_count)),
                Box::from(Const(framerate)),
                Box::from(MaximizeResolution::new()),
            ),
            _ => panic!("only one option can be set to maximized"),
        }
    }

    fn default_single_bench(&self) -> SingleBenchmarkPass {
        SingleBenchmarkPass {
            scene_builder: self.scene_builder,

            input_count: 0,
            output_count: 0,
            framerate: 0,
            output_resolution: Resolution {
                width: 0,
                height: 0,
            },

            input_file: self.input_file.clone(),

            encoder: self.encoder,
            decoder: self.decoder,

            warm_up_time: self.warm_up_time,
            measure_time: self.measure_time,

            error_tolerance_multiplier: self.error_tolerance_multiplier,
        }
    }
}

impl Default for Benchmark {
    fn default() -> Self {
        Self {
            id: "",
            scene_builder: simple_tiles_with_all_inputs,

            input_count: ValueOrMaximized::Value(0),
            output_count: ValueOrMaximized::Value(0),
            framerate: ValueOrMaximized::Value(0),
            output_resolution: ValueOrMaximized::Value(Resolution {
                width: 1,
                height: 1,
            }),

            input_file: InputFile::Mp4(PathBuf::new()), // always override

            encoder: EncoderOptions::Enabled(EncoderPreset::Ultrafast),
            decoder: VideoDecoder::FFmpegH264,

            warm_up_time: Duration::from_secs(10),
            measure_time: Duration::from_secs(20),

            error_tolerance_multiplier: 1.10,
        }
    }
}

impl BenchmarkResult {
    pub fn json(&self) -> serde_json::Value {
        let config = &self.config;
        let (key_result, value_result) = self.get_maximized();
        json!({
          "id": self.config.id,
          "result": {
            key_result: value_result
          },
          "config": {
            "input_count": format!("{:?}", config.input_count),
            "output_count": format!("{:?}", config.output_count),
            "framerate": format!("{:?}", config.framerate),
            "output_resolution": format!("{:?}", config.output_resolution),

            "input_file": format!("{:?}", config.input_file),
            "encoder": format!("{:?}", config.encoder),
            "decoder": format!("{:?}", config.decoder),

            "warm_up_time": config.warm_up_time,
            "measured_time": config.measure_time,
            "error_tolerance_multiplier": config.error_tolerance_multiplier,
          }
        })
    }

    pub fn text(&self) -> String {
        format!(
            "id: {:?}\nresult:{:?}\n",
            &self.config.id,
            self.get_maximized()
        )
    }

    fn get_maximized(&self) -> (&str, String) {
        if let ValueOrMaximized::Maximize = self.config.input_count {
            let value = self.pass.as_ref().map(|p| p.input_count);
            ("input_count", format!("{:?}", value))
        } else if let ValueOrMaximized::Maximize = self.config.output_count {
            let value = self.pass.as_ref().map(|p| p.output_count);
            ("output_count", format!("{:?}", value))
        } else if let ValueOrMaximized::Maximize = self.config.framerate {
            let value = self.pass.as_ref().map(|p| p.framerate);
            ("framerate", format!("{:?}", value))
        } else if let ValueOrMaximized::Maximize = self.config.output_resolution {
            let value = self.pass.as_ref().map(|p| p.output_resolution);
            ("output_resolution", format!("{:?}", value))
        } else {
            ("no-maximize", "true".to_string())
        }
    }
}
