use crate::common_core::prelude as core;
use crate::*;

impl TryFrom<RtmpInput> for core::RegisterInputOptions {
    type Error = TypeError;

    fn try_from(value: RtmpInput) -> Result<Self, Self::Error> {
        let RtmpInput {
            app,
            stream_key,
            required,
            decoder_map,
            side_channel,
        } = value;

        let side_channel = side_channel.unwrap_or_default();

        let h264 = decoder_map
            .as_ref()
            .and_then(|decoders| decoders.get(&InputRtmpCodec::H264))
            .map(|decoder| match decoder {
                RtmpVideoDecoderOptions::FfmpegH264 => Ok(core::VideoDecoderOptions::FfmpegH264),
                RtmpVideoDecoderOptions::VulkanH264 => Ok(core::VideoDecoderOptions::VulkanH264),
            })
            .transpose()?;

        let input_options = core::RtmpServerInputOptions {
            app,
            stream_key,
            decoders: core::RtmpServerInputDecoders { h264 },
            queue_options: core::QueueInputOptions {
                required: required.unwrap_or(false),
                video_side_channel: side_channel.video.unwrap_or(false),
                audio_side_channel: side_channel.audio.unwrap_or(false),
            },
        };

        Ok(core::RegisterInputOptions::RtmpServer(input_options))
    }
}
