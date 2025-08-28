use std::{path::PathBuf, str::FromStr};

use compositor_pipeline::codecs::{FfmpegH264EncoderPreset, VideoDecoderOptions};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumericArgument {
    Maximize,
    Constant(u64),
}

impl std::str::FromStr for NumericArgument {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "maximize" => Ok(NumericArgument::Maximize),
            _ => s
                .parse::<u64>()
                .map(NumericArgument::Constant)
                .map_err(|e| e.to_string()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BenchmarkSuite {
    Full,
    Minimal,
    CpuOptimized,
    EncodersOnly,
    None,
}

impl std::str::FromStr for BenchmarkSuite {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "full" => Ok(BenchmarkSuite::Full),
            "minimal" => Ok(BenchmarkSuite::Minimal),
            "cpu" => Ok(BenchmarkSuite::CpuOptimized),
            "encoders" => Ok(BenchmarkSuite::EncodersOnly),
            "none" => Ok(BenchmarkSuite::None),
            _ => Err("invalid suite name".to_string()),
        }
    }
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
#[clap(rename_all = "snake_case")]
pub enum VideoDecoder {
    FfmpegH264,
    #[cfg(not(target_os = "macos"))]
    VulkanVideoH264,
}

impl From<VideoDecoder> for VideoDecoderOptions {
    fn from(value: VideoDecoder) -> Self {
        match value {
            VideoDecoder::FfmpegH264 => VideoDecoderOptions::FfmpegH264,
            #[cfg(not(target_os = "macos"))]
            VideoDecoder::VulkanVideoH264 => VideoDecoderOptions::VulkanH264,
        }
    }
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
#[clap(rename_all = "snake_case")]
pub enum VideoEncoder {
    FfmpegH264,
    #[cfg(not(target_os = "macos"))]
    VulkanH264,
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

impl From<EncoderPreset> for FfmpegH264EncoderPreset {
    fn from(value: EncoderPreset) -> Self {
        match value {
            EncoderPreset::Ultrafast => FfmpegH264EncoderPreset::Ultrafast,
            EncoderPreset::Superfast => FfmpegH264EncoderPreset::Superfast,
            EncoderPreset::Veryfast => FfmpegH264EncoderPreset::Veryfast,
            EncoderPreset::Faster => FfmpegH264EncoderPreset::Faster,
            EncoderPreset::Fast => FfmpegH264EncoderPreset::Fast,
            EncoderPreset::Medium => FfmpegH264EncoderPreset::Medium,
            EncoderPreset::Slow => FfmpegH264EncoderPreset::Slow,
            EncoderPreset::Slower => FfmpegH264EncoderPreset::Slower,
            EncoderPreset::Veryslow => FfmpegH264EncoderPreset::Veryslow,
            EncoderPreset::Placebo => FfmpegH264EncoderPreset::Placebo,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
                "invalid resolution preset, available options: 144p, 240p, 360p, 480p, 720p, 1080p, 1440p, 2160p, 4320p".to_string(),
            ),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolutionArgument {
    Maximize,
    Iterate,
    Constant(Resolution),
}

impl FromStr for ResolutionArgument {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "iterate" => Ok(ResolutionArgument::Iterate),
            "maximize" => Ok(ResolutionArgument::Maximize),
            _ => s
                .parse::<ResolutionConstant>()
                .map(Resolution::from)
                .map(ResolutionArgument::Constant),
        }
    }
}

/// Only one option can be set to "maximize"
#[derive(Debug, Clone, clap::Parser)]
pub struct Args {
    /// [possible values: none, full or minimal] Run entire benchmark suites, this option will ignore most of the other options.
    #[arg(long, default_value("none"))]
    pub suite: BenchmarkSuite,

    /// [possible values: maximize, number or iterate (to iterate exponentially)]
    #[arg(long, default_value("24"))]
    pub framerate: NumericArgument,

    /// [possible values: maximize, number or iterate (to iterate exponentially)]
    #[arg(long, default_value("maximize"))]
    pub input_count: NumericArgument,

    /// [possible values: maximize, number or iterate (to iterate exponentially)]
    #[arg(long, default_value("1"))]
    pub output_count: NumericArgument,

    /// path to an mp4 file
    #[arg(long)]
    pub input_path: Option<PathBuf>,

    /// [possible values: 4320p, 2160p, 1440p, 1080p, 720p, 480p, 360p, 240p, 144p, <width>x<height>, iterate or maximize]
    #[arg(long, default_value("1080p"))]
    pub output_resolution: ResolutionArgument,

    /// disable encoder
    #[arg(long, default_value("false"))]
    pub disable_encoder: bool,

    #[arg(long, default_value("ffmpeg_h264"))]
    pub video_encoder: VideoEncoder,

    /// FFmpeg_H264 encoder preset
    #[arg(long, default_value("ultrafast"))]
    pub encoder_preset: EncoderPreset,

    /// disable decoder, use raw input
    #[arg(long, default_value("false"))]
    pub disable_decoder: bool,

    #[arg(long, default_value("ffmpeg_h264"))]
    pub video_decoder: VideoDecoder,

    /// print results as json
    #[arg(long, default_value("false"))]
    pub json: bool,

    /// print results as json
    #[arg(long)]
    pub json_file: Option<PathBuf>,
}
