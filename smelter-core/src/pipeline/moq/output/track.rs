//! Translates encoder options into the pair of things every published track
//! needs: the hang catalog entry describing it, and the wire container used to
//! frame its payloads.

use bytes::Bytes;
use hang::catalog as hang_catalog;
use moq_mux::catalog::hang::Container as WireContainer;
use smelter_render::OutputFrameFormat;

use super::init_segment;
use crate::prelude::*;

/// H264 fallback when the encoder gives us no parameter sets to read the real
/// values from. Constrained baseline 3.1 is the safest thing to advertise.
const DEFAULT_H264_PROFILE: (u8, u8, u8) = (0x42, 0x00, 0x1f);

pub(super) fn validate(
    video: &Option<VideoEncoderOptions>,
    container: MoqOutputContainer,
) -> Result<(), MoqClientError> {
    if container != MoqOutputContainer::Cmaf {
        return Ok(());
    }
    let codec = match video {
        Some(VideoEncoderOptions::FfmpegVp8(_)) => VideoCodec::Vp8,
        Some(VideoEncoderOptions::FfmpegVp9(_)) => VideoCodec::Vp9,
        _ => return Ok(()),
    };
    Err(MoqClientError::UnsupportedCodecContainer { codec, container })
}

pub(super) fn video(
    options: &VideoEncoderOptions,
    resolution: Resolution,
    output_format: OutputFrameFormat,
    extradata: Option<Bytes>,
    container: MoqOutputContainer,
) -> Result<(hang_catalog::VideoConfig, WireContainer), MoqClientError> {
    let is_h264 = matches!(
        options,
        VideoEncoderOptions::FfmpegH264(_) | VideoEncoderOptions::VulkanH264(_)
    );
    let extradata = extradata.filter(|data| !data.is_empty());

    // H264 is the only codec whose catalog entry depends on the container: CMAF
    // needs the out-of-band avcC record, Legacy/LOC keep parameter sets inline.
    let (codec, description) = match (is_h264, container) {
        (true, MoqOutputContainer::Cmaf) => {
            let avcc = extradata
                .clone()
                .ok_or(MoqClientError::MissingH264DecoderConfig)?;
            let (profile, constraints, level) = avcc_profile(&avcc)?;
            let codec = hang_catalog::H264 {
                inline: false,
                profile,
                constraints,
                level,
            };
            (codec.into(), Some(avcc))
        }
        (true, _) => {
            let (profile, constraints, level) = DEFAULT_H264_PROFILE;
            let codec = hang_catalog::H264 {
                inline: true,
                profile,
                constraints,
                level,
            };
            (codec.into(), None)
        }
        (false, _) => match options {
            VideoEncoderOptions::FfmpegVp8(_) => (hang_catalog::VideoCodec::VP8, None),
            VideoEncoderOptions::FfmpegVp9(_) => (vp9_codec(output_format).into(), None),
            VideoEncoderOptions::FfmpegH264(_) | VideoEncoderOptions::VulkanH264(_) => {
                unreachable!("handled by the h264 arms above")
            }
        },
    };

    let mut config = hang_catalog::VideoConfig::new(codec);
    config.description = description;
    config.coded_width = Some(resolution.width as u32);
    config.coded_height = Some(resolution.height as u32);
    config.container = match container {
        MoqOutputContainer::Legacy => hang_catalog::Container::Legacy,
        MoqOutputContainer::Loc => hang_catalog::Container::Loc,
        MoqOutputContainer::Cmaf => {
            let init = init_segment::h264(
                config
                    .description
                    .as_deref()
                    .ok_or(MoqClientError::MissingH264DecoderConfig)?,
                resolution,
            )?;
            cmaf_container(init, init_segment::VIDEO_TIMESCALE)
        }
    };

    let wire = WireContainer::try_from(&config.container)
        .map_err(|err| MoqClientError::InitSegmentError(format!("{err}")))?;
    Ok((config, wire))
}

pub(super) fn audio(
    opus: &OpusEncoderOptions,
    container: MoqOutputContainer,
) -> Result<(hang_catalog::AudioConfig, WireContainer), MoqClientError> {
    let channel_count = match opus.channels {
        AudioChannels::Mono => 1,
        AudioChannels::Stereo => 2,
    };
    let mut config = hang_catalog::AudioConfig::new(
        hang_catalog::AudioCodec::Opus,
        opus.sample_rate,
        channel_count,
    );
    config.container = match container {
        MoqOutputContainer::Legacy => hang_catalog::Container::Legacy,
        MoqOutputContainer::Loc => hang_catalog::Container::Loc,
        MoqOutputContainer::Cmaf => {
            let init = init_segment::opus(opus.sample_rate, opus.channels)?;
            cmaf_container(init, opus.sample_rate)
        }
    };

    let wire = WireContainer::try_from(&config.container)
        .map_err(|err| MoqClientError::InitSegmentError(format!("{err}")))?;
    Ok((config, wire))
}

// `timescale` and `track_id` duplicate what's already in `init`; hang emits them
// for players that predate the `init` field.
#[allow(deprecated)]
fn cmaf_container(init: Bytes, timescale: u32) -> hang_catalog::Container {
    hang_catalog::Container::Cmaf {
        init,
        timescale: Some(timescale),
        track_id: Some(init_segment::TRACK_ID),
    }
}

/// AVCDecoderConfigurationRecord: version, profile, compatibility, level.
fn avcc_profile(avcc: &[u8]) -> Result<(u8, u8, u8), MoqClientError> {
    match avcc {
        [_version, profile, constraints, level, ..] => Ok((*profile, *constraints, *level)),
        _ => Err(MoqClientError::MissingH264DecoderConfig),
    }
}

/// it and everything else stays at the 8-bit BT.709-ish defaults.
fn vp9_codec(output_format: OutputFrameFormat) -> hang_catalog::VP9 {
    // VP9 profile 0 is 4:2:0 8-bit; profile 1 covers the other 8-bit subsamplings.
    // Chroma subsampling values are the ones from the VP9 codec string spec.
    let (profile, chroma_subsampling) = match output_format {
        OutputFrameFormat::PlanarYuv422Bytes => (1, 2),
        OutputFrameFormat::PlanarYuv444Bytes => (1, 3),
        _ => (0, 1),
    };
    hang_catalog::VP9 {
        profile,
        level: 31,
        bit_depth: 8,
        chroma_subsampling,
        color_primaries: 1,
        transfer_characteristics: 1,
        matrix_coefficients: 1,
        full_range: false,
    }
}
