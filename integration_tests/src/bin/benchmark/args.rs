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

#[derive(Debug, Clone, Copy)]
pub enum ResolutionPreset {
    Res4320p,
    Res2160p,
    Res1440p,
    Res1080p,
    Res720p,
    Res480p,
    Res360p,
    Res240p,
    Res144p,
}

impl std::str::FromStr for ResolutionPreset {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "4320p" => Ok(ResolutionPreset::Res4320p),
            "2160p" => Ok(ResolutionPreset::Res2160p),
            "1440p" => Ok(ResolutionPreset::Res1440p),
            "1080p" => Ok(ResolutionPreset::Res1080p),
            "720p" => Ok(ResolutionPreset::Res720p),
            "480p" => Ok(ResolutionPreset::Res480p),
            "360p" => Ok(ResolutionPreset::Res360p),
            "240p" => Ok(ResolutionPreset::Res240p),
            "144p" => Ok(ResolutionPreset::Res144p),
            _ => Err(
                "invalid resolution preset, available options: sd, hd, fhd, qhd, uhd".to_string(),
            ),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ResolutionConstant {
    Preset(ResolutionPreset),
    Value(Resolution),
}

impl std::str::FromStr for ResolutionConstant {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.chars()
            .last()
            .ok_or("error while parsing resolution argument".to_string())?
            == 'p'
        {
            let preset = s.parse::<ResolutionPreset>().map_err(|e| e.to_string())?;
            Ok(ResolutionConstant::Preset(preset))
        } else {
            let (width, height) = s
                .split_once("x")
                .ok_or("invalid resolution value, should look like eg. `1920x1080`")?;
            Ok(ResolutionConstant::Value(Resolution {
                width: width.parse::<usize>().map_err(|e| e.to_string())?,
                height: height.parse::<usize>().map_err(|e| e.to_string())?,
            }))
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Resolution {
    pub width: usize,
    pub height: usize,
}

impl From<ResolutionConstant> for Resolution {
    fn from(value: ResolutionConstant) -> Self {
        match value {
            ResolutionConstant::Value(resolution) => resolution,
            ResolutionConstant::Preset(preset) => preset.into(),
        }
    }
}

impl From<ResolutionPreset> for Resolution {
    fn from(value: ResolutionPreset) -> Self {
        match value {
            ResolutionPreset::Res4320p => Resolution {
                width: 7680,
                height: 4320,
            },
            ResolutionPreset::Res2160p => Resolution {
                width: 3840,
                height: 2160,
            },
            ResolutionPreset::Res1440p => Resolution {
                width: 2560,
                height: 1440,
            },
            ResolutionPreset::Res1080p => Resolution {
                width: 1920,
                height: 1080,
            },
            ResolutionPreset::Res720p => Resolution {
                width: 1280,
                height: 720,
            },
            ResolutionPreset::Res480p => Resolution {
                width: 854,
                height: 480,
            },
            ResolutionPreset::Res360p => Resolution {
                width: 640,
                height: 360,
            },
            ResolutionPreset::Res240p => Resolution {
                width: 426,
                height: 240,
            },
            ResolutionPreset::Res144p => Resolution {
                width: 256,
                height: 144,
            },
        }
    }
}

/// Only one option can be set to "maximize"
#[derive(Debug, Clone, clap::Parser)]
pub struct Args {
    /// [possible values: iterate_exp, maximize or a number]
    #[arg(long, default_value("24"))]
    pub framerate: Argument,

    /// [possible values: iterate_exp, maximize or a number]
    #[arg(long, default_value("maximize"))]
    pub input_count: Argument,

    /// [possible values: iterate_exp, maximize or a number]
    #[arg(long, default_value("1"))]
    pub output_count: Argument,

    /// path to .mp4 file or .nv12 if decoder is disabled
    #[arg(long)]
    pub file_path: PathBuf,

    /// [possible values: 4320p, 2160p, 1440p, 1080p, 720p, 480p, 360p, 240p, 144p or `<width>x<height>`]
    #[arg(long, default_value("1080p"))]
    pub output_resolution: ResolutionConstant,

    #[arg(long, default_value("false"))]
    pub disable_encoder: bool,

    #[arg(long, default_value("ultrafast"))]
    pub encoder_preset: EncoderPreset,

    /// warm-up time in seconds
    #[arg(long, default_value("10"))]
    pub warm_up_time: DurationWrapper,

    /// measuring time in seconds
    #[arg(long, default_value("10"))]
    pub measured_time: DurationWrapper,

    /// disable decoder, use raw input with .nv12 file
    #[arg(long, default_value("false"))]
    pub disable_decoder: bool,

    /// resolution of raw input frames, used when decoder disabled
    #[arg(long, required_if_eq("disable_decoder", "true"))]
    pub input_resolution: Option<ResolutionConstant>,

    /// framerate of raw input frames, used when decoder disabled
    #[arg(long, required_if_eq("disable_decoder", "true"))]
    pub input_framerate: Option<u32>,

    /// [possible values: ffmpegh264, vulkan_video_h264 (if your device supports vulkan)]
    #[arg(long, default_value("ffmpeg_h264"))]
    pub video_decoder: VideoDecoder,

    /// in the end of the benchmark the framerate achieved by the compositor is multiplied by this
    /// number, before comparing to the target framerate
    #[arg(long, default_value("1.05"))]
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
            disable_decoder: self.disable_decoder,
            video_decoder: self.video_decoder.into(),
            disable_encoder: self.disable_encoder,
            input_resolution: self.input_resolution.map(|res| res.into()),
            input_framerate: self.input_framerate.map(|fr| fr.into()),
            output_encoder_preset: self.encoder_preset.into(),
            framerate_tolerance_multiplier: self.framerate_tolerance,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SingleBenchConfig {
    pub input_count: u64,
    pub output_count: u64,
    pub framerate: u64,
    pub file_path: PathBuf,
    pub output_resolution: Resolution,
    pub disable_encoder: bool,
    pub output_encoder_preset: ffmpeg_h264::EncoderPreset,
    pub warm_up_time: Duration,
    pub measured_time: Duration,
    pub disable_decoder: bool,
    pub video_decoder: pipeline::VideoDecoder,
    pub input_resolution: Option<Resolution>,
    pub input_framerate: Option<u32>,
    pub framerate_tolerance_multiplier: f64,
}

impl SingleBenchConfig {
    pub fn log_running_config(&self) {
        tracing::info!("config: {:?}", self);
        tracing::info!(
            "checking configuration: framerate: {}, input count: {}, output count: {}",
            self.framerate,
            self.input_count,
            self.output_count
        );
    }

    pub fn log_as_report(&self) {
        print!("{}\t", self.input_count);
        print!("{}\t", self.output_count);
        print!("{}\t", self.framerate);
        print!("{}\t", self.output_resolution.width);
        print!("{}\t", self.output_resolution.height);
        print!("{}\t", self.disable_encoder);
        print!("{:?}\t", self.disable_decoder);
        print!("{:?}\t", self.video_decoder);
        print!("{:?}\t", self.output_encoder_preset);
        print!("{:?}\t", self.warm_up_time);
        print!("{:?}\t", self.measured_time);
        print!("{}\t", self.framerate_tolerance_multiplier);
        println!();
    }

    pub fn log_report_header() {
        println!("input cnt\toutput count\tfps\twidth\theight\tdisable enc\tpreset\tdisable dec\tenc\twarmup\tmeasured\tdec\ttol")
    }
}
