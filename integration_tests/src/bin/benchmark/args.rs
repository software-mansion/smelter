use std::{fs::File, io::Write, path::PathBuf, str::FromStr, time::Duration};

use compositor_pipeline::pipeline::{self, encoder::ffmpeg_h264};
use tracing::error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumericArgument {
    IterateExp,
    Maximize,
    Constant(u64),
}

impl NumericArgument {
    pub fn as_constant(&self) -> Option<u64> {
        if let Self::Constant(v) = self {
            Some(*v)
        } else {
            None
        }
    }
}

impl std::str::FromStr for NumericArgument {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "iterate" => Ok(NumericArgument::IterateExp),
            "maximize" => Ok(NumericArgument::Maximize),
            _ => s
                .parse::<u64>()
                .map(NumericArgument::Constant)
                .map_err(|e| e.to_string()),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ExpIterator(u64);

impl Iterator for ExpIterator {
    type Item = u64;

    fn next(&mut self) -> Option<Self::Item> {
        let tmp = self.0;
        match tmp {
            0 => None,
            _ => {
                self.0 *= 2;
                Some(tmp)
            }
        }
    }
}

impl Default for ExpIterator {
    fn default() -> Self {
        ExpIterator(1)
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

impl ResolutionPreset {
    const ORDER: &[ResolutionPreset] = &[
        ResolutionPreset::Res144p,
        ResolutionPreset::Res240p,
        ResolutionPreset::Res360p,
        ResolutionPreset::Res480p,
        ResolutionPreset::Res720p,
        ResolutionPreset::Res1080p,
        ResolutionPreset::Res1440p,
        ResolutionPreset::Res2160p,
        ResolutionPreset::Res4320p,
    ];

    pub fn iter() -> impl Iterator<Item = ResolutionPreset> {
        ResolutionPreset::ORDER.iter().copied()
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

impl ResolutionArgument {
    pub fn as_constant(&self) -> Option<Resolution> {
        if let Self::Constant(v) = self {
            Some(*v)
        } else {
            None
        }
    }
}

#[derive(Clone, Debug)]
pub enum Argument {
    NumericArgument(NumericArgument),
    ResolutionArgument(ResolutionArgument),
}

impl Argument {
    fn as_numeric(&self) -> Option<NumericArgument> {
        if let Self::NumericArgument(n) = self {
            Some(*n)
        } else {
            None
        }
    }

    fn as_resolution(&self) -> Option<ResolutionArgument> {
        if let Self::ResolutionArgument(n) = self {
            Some(*n)
        } else {
            None
        }
    }

    pub fn is_maximize(&self) -> bool {
        match self {
            Self::NumericArgument(a) => matches!(a, NumericArgument::Maximize),
            Self::ResolutionArgument(a) => matches!(a, ResolutionArgument::Maximize),
        }
    }
}

/// Only one option can be set to "maximize"
#[derive(Debug, Clone, clap::Parser)]
pub struct Args {
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
    pub file_path: PathBuf,

    /// [possible values: 4320p, 2160p, 1440p, 1080p, 720p, 480p, 360p, 240p, 144p, <width>x<height>, iterate or maximize]
    #[arg(long, default_value("1080p"))]
    pub output_resolution: ResolutionArgument,

    /// disable encoder
    #[arg(long, default_value("false"))]
    pub disable_encoder: bool,

    /// FFmpeg_H264 encoder preset
    #[arg(long, default_value("ultrafast"))]
    pub encoder_preset: EncoderPreset,

    /// warm-up time in seconds
    #[arg(long, default_value("10"))]
    pub warm_up_time: DurationWrapper,

    /// measuring time in seconds
    #[arg(long, default_value("10"))]
    pub measured_time: DurationWrapper,

    /// disable decoder, use raw input
    #[arg(long, default_value("false"))]
    pub disable_decoder: bool,

    #[arg(long, default_value("ffmpeg_h264"))]
    pub video_decoder: VideoDecoder,

    /// in the end of the benchmark the framerate achieved by the compositor is multiplied by this number, before comparing to the target framerate
    #[arg(long, default_value("1.05"))]
    pub framerate_tolerance: f64,

    /// if present, result will be saved in specified .csv file
    #[arg(long)]
    pub csv_path: Option<PathBuf>,

    #[arg(skip)]
    pub input_options: Option<RawInputOptions>,
}

#[derive(Debug, Clone, Copy)]
pub struct RawInputOptions {
    pub resolution: Resolution,
    pub framerate: u32,
}

impl Args {
    pub fn arguments(&self) -> Box<[Argument]> {
        vec![
            Argument::NumericArgument(self.framerate),
            Argument::NumericArgument(self.input_count),
            Argument::NumericArgument(self.output_count),
            Argument::ResolutionArgument(self.output_resolution),
        ]
        .into_boxed_slice()
    }

    pub fn with_arguments(&self, arguments: &[Argument]) -> SingleBenchConfig {
        SingleBenchConfig {
            framerate: arguments[0].as_numeric().unwrap().as_constant().unwrap(),
            input_count: arguments[1].as_numeric().unwrap().as_constant().unwrap(),
            output_count: arguments[2].as_numeric().unwrap().as_constant().unwrap(),
            output_resolution: arguments[3].as_resolution().unwrap().as_constant().unwrap(),

            file_path: self.file_path.clone(),
            warm_up_time: self.warm_up_time.0,
            measured_time: self.measured_time.0,
            disable_decoder: self.disable_decoder,
            video_decoder: self.video_decoder.into(),
            disable_encoder: self.disable_encoder,
            output_encoder_preset: self.encoder_preset.into(),
            framerate_tolerance_multiplier: self.framerate_tolerance,
            input_framerate: self.input_options.map(|o| o.framerate),
            input_resolution: self.input_options.map(|o| o.resolution),
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
    const LABELS: &[&str] = &[
        "input count",
        "output count",
        "output fps",
        "output width",
        "output height",
        "disable enc",
        "enc preset",
        "disable dec",
        "dec",
        "warmup time",
        "measured time",
        "tolerance",
    ];

    const COLUMN_WIDTH: usize = 13;

    pub fn log_running_config(&self) {
        tracing::info!("config: {:#?}", self);
    }

    pub fn log_as_report(&self, csv_writer: Option<&mut CsvWriter>) {
        let values = vec![
            self.input_count.to_string(),
            self.output_count.to_string(),
            self.framerate.to_string(),
            self.output_resolution.width.to_string(),
            self.output_resolution.height.to_string(),
            self.disable_encoder.to_string(),
            format!("{:?}", self.output_encoder_preset),
            format!("{:?}", self.disable_decoder),
            format!("{:?}", self.video_decoder),
            format!("{:?}", self.warm_up_time),
            format!("{:?}", self.measured_time),
            self.framerate_tolerance_multiplier.to_string(),
        ];
        match csv_writer {
            Some(writer) => writer.write_line(values),
            None => SingleBenchConfig::print_vec(values),
        }
    }

    pub fn log_report_header(csv_writer: &mut Option<CsvWriter>) {
        let headers = SingleBenchConfig::LABELS
            .iter()
            .map(|s| s.to_string())
            .collect();
        match csv_writer {
            Some(ref mut writer) => writer.write_line(headers),
            None => {
                SingleBenchConfig::print_line();
                SingleBenchConfig::print_vec(headers);
                SingleBenchConfig::print_line();
            }
        }
    }

    fn print_vec(values: Vec<String>) {
        for (i, val) in values.iter().enumerate() {
            if i == 0 {
                print!("| {:<1$} |", val, SingleBenchConfig::COLUMN_WIDTH);
            } else {
                print!(" {:<1$} |", val, SingleBenchConfig::COLUMN_WIDTH);
            }
        }
        println!();
    }

    fn print_line() {
        for i in 0..SingleBenchConfig::LABELS.len() {
            if i == 0 {
                print!("+");
            }
            for _ in 0..SingleBenchConfig::COLUMN_WIDTH + 2 {
                print!("-");
            }
            print!("+")
        }
        println!();
    }
}

pub struct CsvWriter {
    pub file: File,
}

impl CsvWriter {
    pub fn init(path: PathBuf) -> CsvWriter {
        CsvWriter {
            file: File::create(path).unwrap(),
        }
    }

    pub fn write_line(&mut self, values: Vec<String>) {
        let mut line = values.join(",");
        line.push('\n');
        if let Err(err) = self.file.write_all(line.as_bytes()) {
            error!("Error while writing to .csv: {err}");
        }
    }
}
