use std::time::Duration;

use crate::common_pipeline::prelude as pipeline;
use crate::*;

impl TryFrom<Mp4Input> for pipeline::RegisterInputOptions {
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

        const BAD_URL_PATH_SPEC: &str =
            "Exactly one of `url` or `path` has to be specified in a register request for an mp4 input.";

        let source = match (url, path) {
            (Some(_), Some(_)) | (None, None) => {
                return Err(TypeError::new(BAD_URL_PATH_SPEC));
            }
            (Some(url), None) => pipeline::Mp4InputSource::Url(url),
            (None, Some(path)) => pipeline::Mp4InputSource::File(path),
        };

        let queue_options = compositor_pipeline::QueueInputOptions {
            required: required.unwrap_or(false),
            offset: offset_ms.map(|offset_ms| Duration::from_secs_f64(offset_ms / 1000.0)),
        };

        let h264 = decoder_map
            .as_ref()
            .and_then(|decoders| decoders.get(&InputMp4Codec::H264))
            .map(|decoder| match decoder {
                Mp4VideoDecoderOptions::FfmpegH264 => Ok(pipeline::VideoDecoderOptions::FfmpegH264),
                Mp4VideoDecoderOptions::VulkanH264 => Ok(pipeline::VideoDecoderOptions::VulkanH264),
            })
            .transpose()?;

        let video_decoders = pipeline::Mp4InputVideoDecoders { h264 };

        Ok(pipeline::RegisterInputOptions {
            input_options: pipeline::ProtocolInputOptions::Mp4(pipeline::Mp4InputOptions {
                source,
                should_loop: should_loop.unwrap_or(false),
                video_decoders,
            }),
            queue_options,
        })
    }
}
