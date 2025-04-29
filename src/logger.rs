use std::{
    fmt::Debug,
    fs::{self, File},
    str::FromStr,
    sync::OnceLock,
};

use tracing_subscriber::{
    fmt::{self},
    layer::SubscriberExt,
    util::SubscriberInitExt,
    Layer, Registry,
};

use crate::config::{read_config, LoggerConfig, LoggerFormat};

#[derive(Debug, Clone, Copy)]
pub enum FfmpegLogLevel {
    Error,
    Warn,
    Info,
    Debug,
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
            "debug" => Ok(FfmpegLogLevel::Debug),
            "info" => Ok(FfmpegLogLevel::Info),
            "warn" => Ok(FfmpegLogLevel::Warn),
            "error" => Ok(FfmpegLogLevel::Error),
            _ => Err("Invalid FFmpeg logger level."),
        }
    }
}

extern "C" fn ffmpeg_log_callback(
    arg1: *mut libc::c_void,
    log_level: libc::c_int,
    fmt: *const libc::c_char,
    #[cfg(not(target_arch = "aarch64"))] va_list_tag: *mut ffmpeg_next::sys::__va_list_tag,
    #[cfg(target_arch = "aarch64")] va_list_tag: ffmpeg_next::sys::va_list,
) {
    unsafe {
        match ffmpeg_logger_level() {
            FfmpegLogLevel::Error if log_level <= 16 => {
                ffmpeg_next::sys::av_log_default_callback(arg1, log_level, fmt, va_list_tag)
            }
            FfmpegLogLevel::Warn if log_level <= 24 => {
                ffmpeg_next::sys::av_log_default_callback(arg1, log_level, fmt, va_list_tag)
            }
            FfmpegLogLevel::Info if log_level <= 32 => {
                ffmpeg_next::sys::av_log_default_callback(arg1, log_level, fmt, va_list_tag)
            }
            FfmpegLogLevel::Debug if log_level <= 48 => {
                ffmpeg_next::sys::av_log_default_callback(arg1, log_level, fmt, va_list_tag)
            }
            _ => (),
        }
    }
}

pub fn init_logger(opts: LoggerConfig) {
    let env_filter = tracing_subscriber::EnvFilter::new(opts.level.clone());

    let stdout_layer = match opts.format {
        LoggerFormat::Pretty => fmt::Layer::default().pretty().boxed(),
        LoggerFormat::Json => fmt::Layer::default().json().boxed(),
        LoggerFormat::Compact => fmt::Layer::default().compact().boxed(),
    };

    let file_layer = if let Some(log_file) = opts.log_file {
        if log_file.exists() {
            fs::remove_file(&log_file).unwrap()
        };
        fs::create_dir_all(log_file.parent().unwrap()).unwrap();
        let writer = File::create(log_file).unwrap();
        Some(fmt::Layer::default().json().with_writer(writer))
    } else {
        None
    };

    match file_layer {
        Some(file_layer) => Registry::default()
            .with(stdout_layer)
            .with(file_layer)
            .with(env_filter)
            .init(),
        None => Registry::default()
            .with(stdout_layer)
            .with(env_filter)
            .init(),
    }

    unsafe {
        ffmpeg_next::sys::av_log_set_callback(Some(ffmpeg_log_callback));
    }
}
