use std::time::Duration;

use crate::common_core::prelude as core;
use crate::*;

impl TryFrom<Mp4Input> for core::RegisterInputOptions {
    type Error = TypeError;

    fn try_from(value: Mp4Input) -> Result<Self, Self::Error> {
        let Mp4Input {
            url,
            path,
            required,
            offset_ms,
            should_loop,
            decoder_map,
        } = value;

        const BAD_URL_PATH_SPEC: &str = "Exactly one of `url` or `path` has to be specified in a register request for an mp4 input.";

        let source = match (url, path) {
            (Some(_), Some(_)) | (None, None) => {
                return Err(TypeError::new(BAD_URL_PATH_SPEC));
            }
            (Some(url), None) => core::Mp4InputSource::Url(url),
            (None, Some(path)) => core::Mp4InputSource::File(path),
        };

        let queue_options = smelter_core::QueueInputOptions {
            required: required.unwrap_or(false),
            offset: offset_ms.map(|offset_ms| Duration::from_secs_f64(offset_ms / 1000.0)),
        };

        let h264 = decoder_map
            .as_ref()
            .and_then(|decoders| decoders.get(&InputMp4Codec::H264))
            .map(|decoder| match decoder {
                Mp4VideoDecoderOptions::FfmpegH264 => Ok(core::VideoDecoderOptions::FfmpegH264),
                Mp4VideoDecoderOptions::VulkanH264 => Ok(core::VideoDecoderOptions::VulkanH264),
            })
            .transpose()?;

        let video_decoders = core::Mp4InputVideoDecoders { h264 };

        Ok(core::RegisterInputOptions {
            input_options: core::ProtocolInputOptions::Mp4(core::Mp4InputOptions {
                source,
                should_loop: should_loop.unwrap_or(false),
                video_decoders,
            }),
            queue_options,
        })
    }
}
