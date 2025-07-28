use std::{
    env,
    path::{Path, PathBuf},
    str::FromStr,
    sync::Arc,
    time::Duration,
};

use compositor_pipeline::DEFAULT_BUFFER_DURATION;
use compositor_render::{Framerate, RenderingMode, WgpuFeatures};
use rand::Rng;
use tracing::error;

use crate::logger::FfmpegLogLevel;

#[derive(Debug, Clone)]
pub struct Config {
    pub instance_id: String,
    pub logger: LoggerConfig,

    pub api_port: u16,
    pub download_root: Arc<Path>,
    pub stream_fallback_timeout: Duration,
    pub default_buffer_duration: Duration,

    pub ahead_of_time_processing: bool,
    pub run_late_scheduled_events: bool,
    pub never_drop_output_frames: bool,
    pub load_system_fonts: bool,

    pub mixing_sample_rate: u32,
    pub output_framerate: Framerate,

    pub rendering_mode: RenderingMode,
    pub wgpu_force_gpu: bool,
    pub wgpu_required_features: WgpuFeatures,

    pub web_renderer_enable: bool,
    pub web_renderer_gpu_enable: bool,

    pub whip_whep_stun_servers: Arc<Vec<String>>,
    pub whip_whep_server_port: u16,
    pub whip_whep_enable: bool,
}

#[derive(Debug, Clone)]
pub struct LoggerConfig {
    pub ffmpeg_logger_level: FfmpegLogLevel,
    pub format: LoggerFormat,
    pub level: String,
    pub log_file: Option<Arc<Path>>,
}

#[derive(Debug, Copy, Clone)]
pub enum LoggerFormat {
    Pretty,
    Json,
    Compact,
}

impl FromStr for LoggerFormat {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "json" => Ok(LoggerFormat::Json),
            "pretty" => Ok(LoggerFormat::Pretty),
            "compact" => Ok(LoggerFormat::Compact),
            _ => Err("invalid logger format"),
        }
    }
}

pub fn read_config() -> Config {
    try_read_config().expect("Failed to read the config from environment variables.")
}

fn try_read_config() -> Result<Config, String> {
    let api_port = match env::var("SMELTER_API_PORT") {
        Ok(api_port) => api_port
            .parse::<u16>()
            .map_err(|_| "SMELTER_API_PORT has to be valid port number")?,
        Err(_) => 8081,
    };

    let instance_id = match env::var("SMELTER_INSTANCE_ID") {
        Ok(instance_id) => instance_id,
        Err(_) => format!("smelter_{}", rand::thread_rng().gen::<u32>()),
    };

    const DEFAULT_FRAMERATE: Framerate = Framerate { num: 30, den: 1 };
    let output_framerate = match env::var("SMELTER_OUTPUT_FRAMERATE") {
        Ok(framerate) => framerate_from_str(&framerate).unwrap_or(DEFAULT_FRAMERATE),
        Err(_) => DEFAULT_FRAMERATE,
    };

    /// Valid Opus sample rates
    const SUPPORTED_SAMPLE_RATES: [u32; 5] = [8_000, 12_000, 16_000, 24_000, 48_000];
    const DEFAULT_MIXING_SAMPLE_RATE: u32 = 48_000;
    let mixing_sample_rate: u32 = match env::var("SMELTER_MIXING_SAMPLE_RATE") {
        Ok(sample_rate) => {
            let sample_rate = sample_rate
                .parse()
                .map_err(|_| "SMELTER_MIXING_SAMPLE_RATE has to be a valid number")?;

            if SUPPORTED_SAMPLE_RATES.contains(&sample_rate) {
                sample_rate
            } else {
                return Err("SMELTER_MIXING_SAMPLE_RATE has to be a supported sample rate. Supported sample rates are: 8000, 12000, 16000, 24000, 48000".to_string());
            }
        }
        Err(_) => DEFAULT_MIXING_SAMPLE_RATE,
    };

    let wgpu_force_gpu = match env::var("SMELTER_FORCE_GPU") {
        Ok(enable) => bool_env_from_str(&enable).unwrap_or(false),
        Err(_) => false,
    };

    const DEFAULT_STREAM_FALLBACK_TIMEOUT: Duration = Duration::from_millis(500);
    let stream_fallback_timeout = match env::var("SMELTER_STREAM_FALLBACK_TIMEOUT_MS") {
        Ok(timeout_ms) => match timeout_ms.parse::<f64>() {
            Ok(timeout_ms) => Duration::from_secs_f64(timeout_ms / 1000.0),
            Err(_) => {
                println!("CONFIG ERROR: Invalid value provided for \"SMELTER_STREAM_FALLBACK_TIMEOUT_MS\". Falling back to default value 500ms.");
                DEFAULT_STREAM_FALLBACK_TIMEOUT
            }
        },
        Err(_) => DEFAULT_STREAM_FALLBACK_TIMEOUT,
    };

    let logger_level = match env::var("SMELTER_LOGGER_LEVEL") {
        Ok(level) => level,
        Err(_) => "info,wgpu_hal=warn,wgpu_core=warn".to_string(),
    };

    // When building in repo use compact logger
    let default_logger_format = match env::var("CARGO_MANIFEST_DIR") {
        Ok(_) => LoggerFormat::Compact,
        Err(_) => LoggerFormat::Json,
    };
    let logger_format = match env::var("SMELTER_LOGGER_FORMAT") {
        Ok(format) => LoggerFormat::from_str(&format).unwrap_or(default_logger_format),
        Err(_) => default_logger_format,
    };

    let ffmpeg_logger_level = match env::var("SMELTER_FFMPEG_LOGGER_LEVEL") {
        Ok(ffmpeg_log_level) => {
            FfmpegLogLevel::from_str(&ffmpeg_log_level).unwrap_or(FfmpegLogLevel::Warn)
        }
        Err(_) => FfmpegLogLevel::Warn,
    };

    let download_root = env::var("SMELTER_DOWNLOAD_DIR")
        .map(|path| Arc::from(PathBuf::from(path)))
        .unwrap_or_else(|_| Arc::from(env::temp_dir()));

    let web_renderer_enable = match env::var("SMELTER_WEB_RENDERER_ENABLE") {
        Ok(enable) => bool_env_from_str(&enable).unwrap_or(false),
        Err(_) => false,
    };

    let web_renderer_gpu_enable = match env::var("SMELTER_WEB_RENDERER_GPU_ENABLE") {
        Ok(enable) => bool_env_from_str(&enable).unwrap_or(true),
        Err(_) => true,
    };

    let offline_processing: bool = match env::var("SMELTER_OFFLINE_PROCESSING_ENABLE") {
        Ok(enable) => bool_env_from_str(&enable).unwrap_or(false),
        Err(_) => false,
    };

    let ahead_of_time_processing: bool = match env::var("SMELTER_AHEAD_OF_TIME_PROCESSING_ENABLE") {
        Ok(enable) => bool_env_from_str(&enable).unwrap_or(offline_processing),
        Err(_) => offline_processing,
    };

    let never_drop_output_frames: bool = match env::var("SMELTER_NEVER_DROP_OUTPUT_FRAMES") {
        Ok(enable) => bool_env_from_str(&enable).unwrap_or(offline_processing),
        Err(_) => offline_processing,
    };

    let run_late_scheduled_events = match env::var("SMELTER_RUN_LATE_SCHEDULED_EVENTS") {
        Ok(enable) => bool_env_from_str(&enable).unwrap_or(false),
        Err(_) => false,
    };

    let default_wgpu_features: WgpuFeatures =
        WgpuFeatures::SAMPLED_TEXTURE_AND_STORAGE_BUFFER_ARRAY_NON_UNIFORM_INDEXING;
    let wgpu_required_features = match env::var("SMELTER_REQUIRED_WGPU_FEATURES") {
        Ok(required_wgpu_features) => wgpu_features_from_str(&required_wgpu_features).unwrap(),
        Err(_) => default_wgpu_features,
    };

    let default_buffer_duration = match env::var("SMELTER_INPUT_BUFFER_DURATION_MS") {
        Ok(duration) => match duration.parse::<f64>() {
            Ok(duration) => Duration::from_secs_f64(duration / 1000.0),
            Err(_) => {
                println!("CONFIG ERROR: Invalid value provided for \"SMELTER_INPUT_BUFFER_DURATION_MS\". Falling back to default value {DEFAULT_BUFFER_DURATION:?}.");
                DEFAULT_BUFFER_DURATION
            }
        },
        Err(_) => DEFAULT_BUFFER_DURATION,
    };

    let load_system_fonts = match env::var("SMELTER_LOAD_SYSTEM_FONTS") {
        Ok(enable) => bool_env_from_str(&enable).unwrap_or(true),
        Err(_) => true,
    };

    let whip_whep_server_port = match env::var("SMELTER_WHIP_WHEP_SERVER_PORT") {
        Ok(whip_whep_port) => whip_whep_port
            .parse::<u16>()
            .map_err(|_| "SMELTER_WHIP_WHEP_SERVER_PORT has to be valid port number")?,
        Err(_) => 9000,
    };

    let whip_whep_enable = match env::var("SMELTER_START_WHIP_WHEP_SERVER") {
        Ok(enable) => bool_env_from_str(&enable).unwrap_or(true),
        Err(_) => true,
    };

    let log_file = match env::var("SMELTER_LOG_FILE") {
        Ok(path) => Some(Arc::from(PathBuf::from(path))),
        Err(_) => None,
    };

    let default_stun_servers = Arc::new(vec!["stun:stun.l.google.com:19302".to_string()]);

    let stun_servers = match env::var("SMELTER_STUN_SERVERS") {
        Ok(var) => {
            if var.is_empty() {
                error!("empty stun servers env");
                Arc::new(Vec::new())
            } else {
                Arc::new(var.split(',').map(String::from).collect())
            }
        }
        Err(_) => default_stun_servers,
    };

    let rendering_mode = match env::var("SMELTER_FORCE_CPU_OPTIMIZED_RENDERING_MODE") {
        Ok(enable) => match bool_env_from_str(&enable) {
            Some(true) => RenderingMode::CpuOptimized,
            Some(false) | None => RenderingMode::GpuOptimized,
        },
        Err(_) => RenderingMode::GpuOptimized,
    };

    let config = Config {
        instance_id,
        api_port,
        logger: LoggerConfig {
            ffmpeg_logger_level,
            format: logger_format,
            level: logger_level,
            log_file,
        },
        default_buffer_duration,
        ahead_of_time_processing,
        output_framerate,
        run_late_scheduled_events,
        never_drop_output_frames,
        stream_fallback_timeout,
        web_renderer_enable,
        web_renderer_gpu_enable,
        download_root,
        mixing_sample_rate,
        whip_whep_stun_servers: stun_servers,
        wgpu_force_gpu,
        wgpu_required_features,
        load_system_fonts,
        whip_whep_server_port,
        whip_whep_enable,
        rendering_mode,
    };
    Ok(config)
}

fn framerate_from_str(s: &str) -> Result<Framerate, &'static str> {
    const ERROR_MESSAGE: &str = "Framerate needs to be an unsigned integer or a string in the \"NUM/DEN\" format, where NUM and DEN are both unsigned integers.";
    if s.contains('/') {
        let Some((num_str, den_str)) = s.split_once('/') else {
            return Err(ERROR_MESSAGE);
        };
        let num = num_str.parse::<u32>().map_err(|_| ERROR_MESSAGE)?;
        let den = den_str.parse::<u32>().map_err(|_| ERROR_MESSAGE)?;
        Ok(compositor_render::Framerate { num, den })
    } else {
        Ok(compositor_render::Framerate {
            num: s.parse::<u32>().map_err(|_| ERROR_MESSAGE)?,
            den: 1,
        })
    }
}

fn bool_env_from_str(s: &str) -> Option<bool> {
    match s {
        "1" | "true" => Some(true),
        "0" | "false" => Some(false),
        _ => None,
    }
}

fn wgpu_features_from_str(s: &str) -> Result<WgpuFeatures, String> {
    let mut all_features = WgpuFeatures::default();
    for feature in s.split(',') {
        let feature = match feature {
            "DEPTH_CLIP_CONTROL" => WgpuFeatures::DEPTH_CLIP_CONTROL,
            "TIMESTAMP_QUERY" => WgpuFeatures::TIMESTAMP_QUERY,
            "INDIRECT_FIRST_INSTANCE" => WgpuFeatures::INDIRECT_FIRST_INSTANCE,
            "SHADER_F16" => WgpuFeatures::SHADER_F16,
            "BGRA8UNORM_STORAGE" => WgpuFeatures::BGRA8UNORM_STORAGE,
            "FLOAT32_FILTERABLE" => WgpuFeatures::FLOAT32_FILTERABLE,
            "RG11B10UFLOAT_RENDERABLE" => WgpuFeatures::RG11B10UFLOAT_RENDERABLE,
            "DEPTH32FLOAT_STENCIL8" => WgpuFeatures::DEPTH32FLOAT_STENCIL8,
            "TEXTURE_COMPRESSION_BC" => WgpuFeatures::TEXTURE_COMPRESSION_BC,
            "TEXTURE_COMPRESSION_ETC2" => WgpuFeatures::TEXTURE_COMPRESSION_ETC2,
            "TEXTURE_COMPRESSION_ASTC" => WgpuFeatures::TEXTURE_COMPRESSION_ASTC,
            "TEXTURE_FORMAT_16BIT_NORM" => WgpuFeatures::TEXTURE_FORMAT_16BIT_NORM,
            "TEXTURE_COMPRESSION_ASTC_HDR" => WgpuFeatures::TEXTURE_COMPRESSION_ASTC_HDR,
            "TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES" => {
                WgpuFeatures::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES
            }
            "PIPELINE_STATISTICS_QUERY" => WgpuFeatures::PIPELINE_STATISTICS_QUERY,
            "TIMESTAMP_QUERY_INSIDE_PASSES" => WgpuFeatures::TIMESTAMP_QUERY_INSIDE_PASSES,
            "MAPPABLE_PRIMARY_BUFFERS" => WgpuFeatures::MAPPABLE_PRIMARY_BUFFERS,
            "TEXTURE_BINDING_ARRAY" => WgpuFeatures::TEXTURE_BINDING_ARRAY,
            "BUFFER_BINDING_ARRAY" => WgpuFeatures::BUFFER_BINDING_ARRAY,
            "STORAGE_RESOURCE_BINDING_ARRAY" => WgpuFeatures::STORAGE_RESOURCE_BINDING_ARRAY,
            "SAMPLED_TEXTURE_AND_STORAGE_BUFFER_ARRAY_NON_UNIFORM_INDEXING" => {
                WgpuFeatures::SAMPLED_TEXTURE_AND_STORAGE_BUFFER_ARRAY_NON_UNIFORM_INDEXING
            }
            "STORAGE_TEXTURE_ARRAY_NON_UNIFORM_INDEXING" => {
                WgpuFeatures::STORAGE_TEXTURE_ARRAY_NON_UNIFORM_INDEXING
            }
            "PARTIALLY_BOUND_BINDING_ARRAY" => WgpuFeatures::PARTIALLY_BOUND_BINDING_ARRAY,
            "MULTI_DRAW_INDIRECT" => WgpuFeatures::MULTI_DRAW_INDIRECT,
            "MULTI_DRAW_INDIRECT_COUNT" => WgpuFeatures::MULTI_DRAW_INDIRECT_COUNT,
            "PUSH_CONSTANTS" => WgpuFeatures::PUSH_CONSTANTS,
            "ADDRESS_MODE_CLAMP_TO_ZERO" => WgpuFeatures::ADDRESS_MODE_CLAMP_TO_ZERO,
            "ADDRESS_MODE_CLAMP_TO_BORDER" => WgpuFeatures::ADDRESS_MODE_CLAMP_TO_BORDER,
            "POLYGON_MODE_LINE" => WgpuFeatures::POLYGON_MODE_LINE,
            "POLYGON_MODE_POINT" => WgpuFeatures::POLYGON_MODE_POINT,
            "CONSERVATIVE_RASTERIZATION" => WgpuFeatures::CONSERVATIVE_RASTERIZATION,
            "VERTEX_WRITABLE_STORAGE" => WgpuFeatures::VERTEX_WRITABLE_STORAGE,
            "CLEAR_TEXTURE" => WgpuFeatures::CLEAR_TEXTURE,
            "SPIRV_SHADER_PASSTHROUGH" => WgpuFeatures::SPIRV_SHADER_PASSTHROUGH,
            "MULTIVIEW" => WgpuFeatures::MULTIVIEW,
            "VERTEX_ATTRIBUTE_64BIT" => WgpuFeatures::VERTEX_ATTRIBUTE_64BIT,
            "TEXTURE_FORMAT_NV12" => WgpuFeatures::TEXTURE_FORMAT_NV12,
            "EXPERIMENTAL_RAY_TRACING_ACCELERATION_STRUCTURE" => {
                WgpuFeatures::EXPERIMENTAL_RAY_TRACING_ACCELERATION_STRUCTURE
            }
            "EXPERIMENTAL_RAY_QUERY" => WgpuFeatures::EXPERIMENTAL_RAY_QUERY,
            "SHADER_F64" => WgpuFeatures::SHADER_F64,
            "SHADER_I16" => WgpuFeatures::SHADER_I16,
            "SHADER_PRIMITIVE_INDEX" => WgpuFeatures::SHADER_PRIMITIVE_INDEX,
            "SHADER_EARLY_DEPTH_TEST" => WgpuFeatures::SHADER_EARLY_DEPTH_TEST,
            "DUAL_SOURCE_BLENDING" => WgpuFeatures::DUAL_SOURCE_BLENDING,
            "" => WgpuFeatures::default(),
            feature => {
                return Err(format!("Unknown wgpu feature \"{feature}\""));
            }
        };
        all_features.set(feature, true)
    }
    Ok(all_features)
}
