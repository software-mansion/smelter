//! Translates encoder options into the pair of things every published track
//! needs: the hang catalog entry describing it, and the wire container used to
//! frame its payloads.

use bytes::Bytes;
use hang::catalog as hang_catalog;
use moq_mux::catalog::hang::Container as WireContainer;
use smelter_render::{Framerate, OutputFrameFormat};

use crate::{
    pipeline::moq::output::cmaf_init_segment::{
        self, aac_cmaf_init_segment, h264_cmaf_init_segment, opus_cmaf_init_segment,
        vp8_cmaf_init_segment, vp9_cmaf_init_segment,
    },
    prelude::*,
};

/// H264 fallback when the encoder gives us no parameter sets to read the real
/// values from. Constrained baseline 3.0 is the safest thing to advertise.
/// With this setting stream should never be falsely rejected, however may fail to decode.
const DEFAULT_H264_PROFILE: (u8, u8, u8) = (0x42, 0xe0, 0x1e);

pub(super) fn video_catalog_entry(
    options: &VideoEncoderOptions,
    resolution: Resolution,
    output_format: OutputFrameFormat,
    framerate: Framerate,
    extradata: Option<Bytes>,
    container: MoqOutputContainer,
) -> Result<(hang_catalog::VideoConfig, WireContainer), MoqClientError> {
    let extradata = extradata.filter(|data| !data.is_empty());

    // H264 is the only codec whose catalog entry depends on the container: CMAF
    // needs the out-of-band avcC record, Legacy/LOC keep parameter sets inline.
    let (codec, description) = match options {
        VideoEncoderOptions::FfmpegH264(_) | VideoEncoderOptions::VulkanH264(_) => {
            match container {
                MoqOutputContainer::Cmaf => {
                    let avcc = extradata
                        .clone()
                        .ok_or(MoqClientError::MissingH264EncoderConfig)?;
                    let (profile, constraints, level) = avcc_profile(&avcc)?;
                    let codec = hang_catalog::H264 {
                        inline: false,
                        profile,
                        constraints,
                        level,
                    };
                    (codec.into(), Some(avcc))
                }
                _ => {
                    let (profile, constraints, level) = DEFAULT_H264_PROFILE;
                    let codec = hang_catalog::H264 {
                        inline: true,
                        profile,
                        constraints,
                        level,
                    };
                    (codec.into(), None)
                }
            }
        }
        VideoEncoderOptions::FfmpegVp8(_) => (hang_catalog::VideoCodec::VP8, None),
        VideoEncoderOptions::FfmpegVp9(_) => {
            (vp9_codec(output_format, resolution, framerate).into(), None)
        }
    };

    let mut config = hang_catalog::VideoConfig::new(codec);
    config.description = description;
    config.coded_width = Some(resolution.width as u32);
    config.coded_height = Some(resolution.height as u32);
    config.container = match container {
        MoqOutputContainer::Legacy => hang_catalog::Container::Legacy,
        MoqOutputContainer::Loc => hang_catalog::Container::Loc,
        MoqOutputContainer::Cmaf => {
            let init = match &config.codec {
                hang_catalog::VideoCodec::H264(_) => h264_cmaf_init_segment(
                    config
                        .description
                        .as_deref()
                        .ok_or(MoqClientError::MissingH264EncoderConfig)?,
                    resolution,
                )?,
                hang_catalog::VideoCodec::VP8 => vp8_cmaf_init_segment(resolution)?,
                hang_catalog::VideoCodec::VP9(vp9) => vp9_cmaf_init_segment(vp9, resolution)?,
                _ => unreachable!("codec is built from the encoder options above"),
            };
            cmaf_container(init, cmaf_init_segment::VIDEO_TIMESCALE)
        }
    };

    let wire = WireContainer::try_from(&config.container)
        .map_err(|err| MoqClientError::InitSegmentError(format!("{err}")))?;
    Ok((config, wire))
}

pub(super) fn audio_catalog_entry(
    options: &AudioEncoderOptions,
    extradata: Option<Bytes>,
    container: MoqOutputContainer,
) -> Result<(hang_catalog::AudioConfig, WireContainer), MoqClientError> {
    match options {
        AudioEncoderOptions::Opus(opus) => opus_audio_catalog_entry(opus, extradata, container),
        AudioEncoderOptions::FdkAac(aac) => aac_audio_catalog_entry(aac, extradata, container),
    }
}

fn opus_audio_catalog_entry(
    opus: &OpusEncoderOptions,
    extradata: Option<Bytes>,
    container: MoqOutputContainer,
) -> Result<(hang_catalog::AudioConfig, WireContainer), MoqClientError> {
    let channel_count = channel_count(opus.channels);
    let mut config = hang_catalog::AudioConfig::new(
        hang_catalog::AudioCodec::Opus,
        opus.sample_rate,
        channel_count,
    );
    config.container = match container {
        MoqOutputContainer::Legacy => hang_catalog::Container::Legacy,
        MoqOutputContainer::Loc => hang_catalog::Container::Loc,
        MoqOutputContainer::Cmaf => {
            let init = opus_cmaf_init_segment(opus.sample_rate, extradata, opus.channels)?;
            cmaf_container(init, opus.sample_rate)
        }
    };

    let wire = WireContainer::try_from(&config.container)
        .map_err(|err| MoqClientError::InitSegmentError(format!("{err}")))?;
    Ok((config, wire))
}

fn aac_audio_catalog_entry(
    aac: &FdkAacEncoderOptions,
    extradata: Option<Bytes>,
    container: MoqOutputContainer,
) -> Result<(hang_catalog::AudioConfig, WireContainer), MoqClientError> {
    let channel_count = channel_count(aac.channels);

    // CMAF carries the AudioSpecificConfig out-of-band (catalog `description` +
    // `mp4a`/`esds` in the init segment), so the profile is read from the real
    // ASC. Legacy/LOC use self-describing ADTS: the encoder is always AAC-LC
    // (profile 2) and no description is published.
    let (codec, description) = match container {
        MoqOutputContainer::Cmaf => {
            let asc = extradata
                .filter(|data| !data.is_empty())
                .ok_or(MoqClientError::MissingAacEncoderConfig)?;
            let profile = AacAudioSpecificConfig::parse_from(&asc)
                .map_err(|err| {
                    MoqClientError::InitSegmentError(format!("invalid AudioSpecificConfig: {err}"))
                })?
                .profile;
            (hang_catalog::AAC { profile }, Some(asc))
        }
        MoqOutputContainer::Legacy | MoqOutputContainer::Loc => {
            (hang_catalog::AAC { profile: 2 }, None)
        }
    };

    let mut config = hang_catalog::AudioConfig::new(codec, aac.sample_rate, channel_count);
    config.description = description.clone();
    config.container = match container {
        MoqOutputContainer::Legacy => hang_catalog::Container::Legacy,
        MoqOutputContainer::Loc => hang_catalog::Container::Loc,
        MoqOutputContainer::Cmaf => {
            let asc = description.ok_or(MoqClientError::MissingAacEncoderConfig)?;
            cmaf_container(aac_cmaf_init_segment(&asc)?, aac.sample_rate)
        }
    };

    let wire = WireContainer::try_from(&config.container)
        .map_err(|err| MoqClientError::InitSegmentError(format!("{err}")))?;
    Ok((config, wire))
}

fn channel_count(channels: AudioChannels) -> u32 {
    match channels {
        AudioChannels::Mono => 1,
        AudioChannels::Stereo => 2,
    }
}

// `timescale` and `track_id` duplicate what's already in `init`; hang emits them
// for players that predate the `init` field.
#[allow(deprecated)]
fn cmaf_container(init: Bytes, timescale: u32) -> hang_catalog::Container {
    hang_catalog::Container::Cmaf {
        init,
        timescale: Some(timescale),
        track_id: Some(cmaf_init_segment::TRACK_ID),
    }
}

/// AVCDecoderConfigurationRecord: version, profile, compatibility, level.
fn avcc_profile(avcc: &[u8]) -> Result<(u8, u8, u8), MoqClientError> {
    match avcc {
        [_version, profile, constraints, level, ..] => Ok((*profile, *constraints, *level)),
        _ => Err(MoqClientError::MissingH264EncoderConfig),
    }
}

/// it and everything else stays at the 8-bit BT.709-ish defaults.
fn vp9_codec(
    output_format: OutputFrameFormat,
    resolution: Resolution,
    framerate: Framerate,
) -> hang_catalog::VP9 {
    // VP9 profile 0 is 4:2:0 8-bit; profile 1 covers the other 8-bit subsamplings.
    // Chroma subsampling values are the ones from the VP9 codec string spec.
    let (profile, chroma_subsampling) = match output_format {
        OutputFrameFormat::PlanarYuv422Bytes => (1, 2),
        OutputFrameFormat::PlanarYuv444Bytes => (1, 3),
        _ => (0, 1),
    };

    hang_catalog::VP9 {
        profile,
        level: vp9_level(resolution, framerate),
        bit_depth: 8,
        chroma_subsampling,
        ..Default::default()
    }
}

/// VP9 level as the decimal number used in the `vp09.` codec string, i.e. 10 for
/// level 1, 41 for level 4.1. 0 means "unknown", which is what the spec table has
/// us fall back to for anything past level 6.2.
fn vp9_level(resolution: Resolution, framerate: Framerate) -> u8 {
    // (max luma sample rate, max luma picture size, level), ordered by level so
    // the first row that fits is the lowest level that can carry the stream.
    const LEVELS: [(u64, u64, u8); 14] = [
        (829_440, 36_864, 10),
        (2_764_800, 73_728, 11),
        (4_608_000, 122_880, 20),
        (9_216_000, 245_760, 21),
        (20_736_000, 552_960, 30),
        (36_864_000, 983_040, 31),
        (83_558_400, 2_228_224, 40),
        (160_432_128, 2_228_224, 41),
        (311_951_360, 8_912_896, 50),
        (588_251_136, 8_912_896, 51),
        (1_176_502_272, 8_912_896, 52),
        (1_176_502_272, 35_651_584, 60),
        (2_353_004_544, 35_651_584, 61),
        (4_706_009_088, 35_651_584, 62),
    ];

    let picture_size = (resolution.width * resolution.height) as u64;
    if picture_size == 0 || framerate.den == 0 {
        return 0;
    }
    let sample_rate = picture_size * framerate.num as u64 / framerate.den as u64;

    LEVELS
        .iter()
        .find(|(max_sample_rate, max_picture_size, _)| {
            sample_rate <= *max_sample_rate && picture_size <= *max_picture_size
        })
        .map_or(0, |(_, _, level)| *level)
}
