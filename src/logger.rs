use std::{
    fmt::Debug,
    fs::{self, File},
    str::FromStr,
    sync::OnceLock,
};

use tracing_subscriber::{
    Layer, Registry,
    fmt::{self},
    layer::SubscriberExt,
    util::SubscriberInitExt,
};

use crate::config::{LoggerConfig, LoggerFormat, read_config};

#[derive(Debug, Clone, Copy)]
pub enum FfmpegLogLevel {
    Error,
    Warn,
    Info,
    Verbose,
    Debug,
    Trace,
}

impl FfmpegLogLevel {
    fn into_i32(self) -> i32 {
        match self {
            FfmpegLogLevel::Error => 16,
            FfmpegLogLevel::Warn => 24,
            FfmpegLogLevel::Info => 32,
            FfmpegLogLevel::Verbose => 40,
            FfmpegLogLevel::Debug => 48,
            FfmpegLogLevel::Trace => 56,
        }
    }
}

fn ffmpeg_logger_level() -> FfmpegLogLevel {
    static LOG_LEVEL: OnceLock<FfmpegLogLevel> = OnceLock::new();

    // This will read config second time
    *LOG_LEVEL.get_or_init(|| read_config().logger.ffmpeg_logger_level)
}

impl FromStr for FfmpegLogLevel {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "trace" => Ok(FfmpegLogLevel::Trace),
            "debug" => Ok(FfmpegLogLevel::Debug),
            "verbose" => Ok(FfmpegLogLevel::Verbose),
            "info" => Ok(FfmpegLogLevel::Info),
            "warn" => Ok(FfmpegLogLevel::Warn),
            "error" => Ok(FfmpegLogLevel::Error),
            _ => Err("Invalid FFmpeg logger level."),
        }
    }
}

pub fn init_logger(opts: LoggerConfig) {
    let stdio_filter = tracing_subscriber::EnvFilter::new(opts.stdio_level.clone());
    let stdio_layer = match opts.format {
        LoggerFormat::Pretty => fmt::Layer::default().pretty().boxed(),
        LoggerFormat::Json => fmt::Layer::default().json().boxed(),
        LoggerFormat::Compact => fmt::Layer::default().compact().boxed(),
    }
    .with_filter(stdio_filter);

    let file_layer = if let Some(log_file) = opts.log_file {
        if log_file.exists() {
            fs::remove_file(&log_file).unwrap()
        };
        fs::create_dir_all(log_file.parent().unwrap()).unwrap();
        let writer = File::create(log_file).unwrap();
        let filter = tracing_subscriber::EnvFilter::new(opts.file_level.clone());
        Some(
            fmt::Layer::default()
                .json()
                .with_writer(writer)
                .with_filter(filter),
        )
    } else {
        None
    };

    match file_layer {
        Some(file_layer) => Registry::default()
            .with(stdio_layer)
            .with(file_layer)
            .init(),
        None => Registry::default().with(stdio_layer).init(),
    }

    unsafe {
        ffmpeg_next::sys::av_log_set_level(ffmpeg_logger_level().into_i32());
    }
}
