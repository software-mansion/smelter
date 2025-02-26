use std::{path::PathBuf, time::Duration};

use compositor_pipeline::pipeline::{self, encoder::ffmpeg_h264};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Argument {
    IterateExp,
    Maximize,
    Constant(u64),
}

impl Argument {
    pub fn as_constant(&self) -> Option<u64> {
        if let Self::Constant(v) = self {
            Some(*v)
        } else {
            None
        }
    }
}

impl std::str::FromStr for Argument {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s == "iterate_exp" {
            return Ok(Argument::IterateExp);
        }

        if s == "maximize" {
            return Ok(Argument::Maximize);
        }

        s.parse::<u64>()
            .map(Argument::Constant)
            .map_err(|e| e.to_string())
    }
}

#[derive(Debug, Clone, Copy)]
pub struct DurationWrapper(pub Duration);

impl std::str::FromStr for DurationWrapper {
    type Err = std::num::ParseFloatError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse::<f64>()
            .map(|f| DurationWrapper(Duration::from_secs_f64(f)))
    }
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
#[clap(rename_all = "snake_case")]
pub enum VideoDecoder {
    FfmpegH264,
    #[cfg(not(target_os = "macos"))]
    VulkanVideoH264,
}

impl From<VideoDecoder> for pipeline::VideoDecoder {
    fn from(value: VideoDecoder) -> Self {
        match value {
            VideoDecoder::FfmpegH264 => pipeline::VideoDecoder::FFmpegH264,
            #[cfg(not(target_os = "macos"))]
            VideoDecoder::VulkanVideoH264 => pipeline::VideoDecoder::VulkanVideoH264,
        }
    }
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
#[clap(rename_all = "snake_case")]
pub enum EncoderPreset {
    Ultrafast,
    Superfast,
    Veryfast,
    Faster,
    Fast,
    Medium,
    Slow,
    Slower,
    Veryslow,
    Placebo,
}

impl From<EncoderPreset> for ffmpeg_h264::EncoderPreset {
    fn from(value: EncoderPreset) -> Self {
        match value {
            EncoderPreset::Ultrafast => ffmpeg_h264::EncoderPreset::Ultrafast,
            EncoderPreset::Superfast => ffmpeg_h264::EncoderPreset::Superfast,
            EncoderPreset::Veryfast => ffmpeg_h264::EncoderPreset::Veryfast,
            EncoderPreset::Faster => ffmpeg_h264::EncoderPreset::Faster,
            EncoderPreset::Fast => ffmpeg_h264::EncoderPreset::Fast,
            EncoderPreset::Medium => ffmpeg_h264::EncoderPreset::Medium,
            EncoderPreset::Slow => ffmpeg_h264::EncoderPreset::Slow,
            EncoderPreset::Slower => ffmpeg_h264::EncoderPreset::Slower,
            EncoderPreset::Veryslow => ffmpeg_h264::EncoderPreset::Veryslow,
            EncoderPreset::Placebo => ffmpeg_h264::EncoderPreset::Placebo,
        }
    }
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
#[clap(rename_all = "snake_case")]
pub enum ResolutionPreset {
    Uhd,
    Qhd,
    Fhd,
    Hd,
    Sd,
}

impl std::str::FromStr for ResolutionPreset {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "uhd" => Ok(ResolutionPreset::Uhd),
            "qhd" => Ok(ResolutionPreset::Qhd),
            "fhd" => Ok(ResolutionPreset::Fhd),
            "hd" => Ok(ResolutionPreset::Hd),
            "sd" => Ok(ResolutionPreset::Sd),
            _ => Err(
                "invalid resolution preset, available options: sd, hd, fhd, qhd, uhd".to_string(),
            ),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ResolutionConstant {
    Preset(ResolutionPreset),
    Value(u32, u32),
}

impl std::str::FromStr for ResolutionConstant {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.as_bytes()
            .first()
            .ok_or("error while parsing resolution argument".to_string())?
            .is_ascii_digit()
        {
            let (width, height) = s
                .split_once("x")
                .ok_or("invalid resolution value, should look like eg. `1920x1080`")?;
            Ok(ResolutionConstant::Value(
                width.parse::<u32>().map_err(|e| e.to_string())?,
                height.parse::<u32>().map_err(|e| e.to_string())?,
            ))
        } else {
            let preset = s.parse::<ResolutionPreset>().map_err(|e| e.to_string())?;
            Ok(ResolutionConstant::Preset(preset))
        }
    }
}

#[derive(Debug)]
pub struct Resolution {
    pub width: u32,
    pub height: u32,
}

impl From<ResolutionConstant> for Resolution {
    fn from(value: ResolutionConstant) -> Self {
        match value {
            ResolutionConstant::Value(width, height) => Resolution { width, height },
            ResolutionConstant::Preset(preset) => preset.into(),
        }
    }
}

impl From<ResolutionPreset> for Resolution {
    fn from(value: ResolutionPreset) -> Self {
        match value {
            ResolutionPreset::Uhd => Resolution {
                width: 3840,
                height: 2160,
            },
            ResolutionPreset::Qhd => Resolution {
                width: 2560,
                height: 1440,
            },
            ResolutionPreset::Fhd => Resolution {
                width: 1920,
                height: 1080,
            },
            ResolutionPreset::Hd => Resolution {
                width: 1280,
                height: 720,
            },
            ResolutionPreset::Sd => Resolution {
                width: 640,
                height: 480,
            },
        }
    }
}

/// Only one option can be set to "maximize"
#[derive(Debug, Clone, clap::Parser)]
pub struct Args {
    /// [possible values: iterate_exp, maximize or a number]
    #[arg(long)]
    pub framerate: Argument,

    /// [possible values: iterate_exp, maximize or a number]
    #[arg(long)]
    pub input_count: Argument,

    /// [possible values: iterate_exp, maximize or a number]
    #[arg(long)]
    pub output_count: Argument,

    #[arg(long)]
    pub file_path: PathBuf,

    /// [possible values: uhd, qhd, fhd, hd, sd or `<width>x<height>`]
    #[arg(long)]
    pub output_resolution: ResolutionConstant,

    #[arg(long)]
    pub disable_encoder: bool,

    #[arg(long, global = true, required_unless_present("disable_encoder"))]
    pub encoder_preset: Option<EncoderPreset>,

    /// warm-up time in seconds
    #[arg(long)]
    pub warm_up_time: DurationWrapper,

    /// measuring time in seconds
    #[arg(long)]
    pub measured_time: DurationWrapper,

    #[arg(long)]
    pub video_decoder: VideoDecoder,

    /// in the end of the benchmark the framerate achieved by the compositor is multiplied by this
    /// number, before comparing to the target framerate
    #[arg(long)]
    pub framerate_tolerance: f64,
}

impl Args {
    pub fn arguments(&self) -> Box<[Argument]> {
        vec![self.framerate, self.input_count, self.output_count].into_boxed_slice()
    }

    pub fn with_arguments(&self, arguments: &[Argument]) -> SingleBenchConfig {
        SingleBenchConfig {
            framerate: arguments[0].as_constant().unwrap(),
            input_count: arguments[1].as_constant().unwrap(),
            output_count: arguments[2].as_constant().unwrap(),

            file_path: self.file_path.clone(),
            output_resolution: self.output_resolution.into(),
            warm_up_time: self.warm_up_time.0,
            measured_time: self.measured_time.0,
            video_decoder: self.video_decoder.into(),
            disable_encoder: self.disable_encoder,
            output_encoder_preset: self.encoder_preset.map(|preset| preset.into()),
            framerate_tolerance_multiplier: self.framerate_tolerance,
        }
    }
}

#[derive(Debug)]
pub struct SingleBenchConfig {
    pub input_count: u64,
    pub output_count: u64,
    pub framerate: u64,
    pub file_path: PathBuf,
    pub output_resolution: Resolution,
    pub disable_encoder: bool,
    pub output_encoder_preset: Option<ffmpeg_h264::EncoderPreset>,
    pub warm_up_time: Duration,
    pub measured_time: Duration,
    pub video_decoder: pipeline::VideoDecoder,
    pub framerate_tolerance_multiplier: f64,
}

impl SingleBenchConfig {
    pub fn log_running_config(&self) {
        tracing::info!("config: {:?}", self);
        tracing::info!(
            "checking configuration: framerate: {}, decoder count: {}, encoder count: {}",
            self.framerate,
            self.input_count,
            self.output_count
        );
    }

    pub fn log_as_report(&self) {
        print!("{}\t", self.input_count);
        print!("{}\t", self.output_count);
        print!("{}\t", self.framerate);
        print!("{:?}\t", self.output_resolution);
        print!("{:?}\t", self.disable_encoder);
        print!("{:?}\t", self.output_encoder_preset);
        print!("{:?}\t", self.warm_up_time);
        print!("{:?}\t", self.measured_time);
        print!("{:?}\t", self.video_decoder);
        print!("{}\t", self.framerate_tolerance_multiplier);
        println!();
    }

    pub fn log_report_header() {
        println!("dec cnt\tfps\twidth\theight\tpreset\twarmup\tmeasured\tdec\ttol")
    }
}
