use std::{sync::Arc, time::Duration};

use compositor_pipeline::{graphics_context::GraphicsContext, FfmpegH264EncoderPreset};
use serde_json::{json, Value as JsonValue};
use tracing::{error, info};

use crate::{
    benchmark_pass::SingleBenchmarkPass,
    maximize_iter::{MaximizeIter, MaximizeU64},
};

#[derive(Debug, Clone, Copy)]
pub enum EncoderOptions {
    Enabled(FfmpegH264EncoderPreset),
    Disabled,
}

type BenchPassBuilder = Box<dyn (Fn(u64) -> SingleBenchmarkPass)>;

#[derive(Clone)]
pub struct Benchmark {
    pub id: &'static str,
    pub bench_pass_builder: Arc<BenchPassBuilder>,
}

pub struct BenchmarkResult {
    pub id: &'static str,
    pub pass: Option<SingleBenchmarkPass>,
}

impl Benchmark {
    pub fn run(&self, ctx: &GraphicsContext) -> BenchmarkResult {
        let mut maximizer = MaximizeU64::new(1);

        let mut last_valid_pass = None;
        let mut prev_success = true;
        while let Some(current) = maximizer.next(prev_success) {
            let mut pass = (*self.bench_pass_builder)(current);

            // add first 30 frames to warm_up_time
            pass.warm_up_time += Duration::from_secs_f64(30.0 / pass.framerate as f64);

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
            info!("{}: ({:?})", status_str, pass);
            if prev_success {
                last_valid_pass = Some(pass);
            }
        }

        BenchmarkResult {
            id: self.id,
            pass: last_valid_pass,
        }
    }
}

impl BenchmarkResult {
    pub fn json(&self) -> JsonValue {
        json!({
          "id": self.id,
          "config": match &self.pass {
            Some(pass) => json!({
              "input_count": format!("{:?}", pass.input_count),
              "output_count": format!("{:?}", pass.output_count),
              "framerate": format!("{:?}", pass.framerate),
              "output_resolution": format!("{:?}", pass.output_resolution),
              "input_file": format!("{:?}", pass.input_file),
              "encoder": format!("{:?}", pass.encoder),
              "decoder": format!("{:?}", pass.decoder),
              "warm_up_time": pass.warm_up_time,
            }),
            None => serde_json::Value::Null,
          }
        })
    }

    pub fn text(&self) -> String {
        format!("id: {:?}\nresult: {:?}\n", &self.id, &self.pass)
    }
}
