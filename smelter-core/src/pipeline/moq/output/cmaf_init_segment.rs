//! CMAF init segment (ftyp+moov) construction.
//!
//! moq-mux keeps its own builders private, so we assemble the equivalent atoms
//! here. Only the codecs we allow with the CMAF container are covered:
//! H264/VP8/VP9 on the video side and Opus or AAC on the audio side.
//!
//! `fmp4::Wire` only reads `mdhd.timescale`, `tkhd.track_id` and `stsd` back out
//! of the trak, but the segment is also handed to players verbatim, so it has to
//! be a well-formed init segment.

use bytes::Bytes;
use mp4_atom::{Atom, Encode};

use crate::prelude::*;

/// Timescale for video tracks. Encoder timestamps are microseconds, so any
/// timescale works; 90 kHz is the MPEG convention.
pub(super) const VIDEO_TIMESCALE: u32 = 90_000;

/// Each MoQ track carries exactly one media track, so the id is always the same.
pub(super) const TRACK_ID: u32 = 1;

pub(super) fn h264_cmaf_init_segment(
    extradata: &[u8],
    resolution: Resolution,
) -> Result<Bytes, MoqClientError> {
    let mut cursor = std::io::Cursor::new(extradata);
    let avcc = mp4_atom::Avcc::decode_body(&mut cursor)
        .map_err(|err| MoqClientError::InitSegmentError(format!("invalid avcC record: {err}")))?;

    let sample_entry = mp4_atom::Codec::from(mp4_atom::Avc1 {
        visual: visual(resolution),
        avcc,
        ..Default::default()
    });

    video_cmaf_init_segment(sample_entry, resolution)
}

/// VP8 carries no out-of-band configuration, but the VP Codec ISO-BMFF binding
/// still requires a `vpcC` box. VP8 is always 8-bit 4:2:0, so we emit the same
/// standard placeholder values as moq-mux's vp08 synthesis.
pub(super) fn vp8_cmaf_init_segment(resolution: Resolution) -> Result<Bytes, MoqClientError> {
    let sample_entry = mp4_atom::Codec::from(mp4_atom::Vp08 {
        visual: visual(resolution),
        vpcc: mp4_atom::VpcC {
            profile: 0,
            level: 0,
            bit_depth: 8,
            ..Default::default()
        },
        ..Default::default()
    });

    video_cmaf_init_segment(sample_entry, resolution)
}

/// Synthesizes the `vpcC` box field-for-field from the same VP9 parameters used
/// to build the catalog `vp09.*` codec string, so the init segment and the codec
/// string can never diverge.
pub(super) fn vp9_cmaf_init_segment(
    vp9: &hang::catalog::VP9,
    resolution: Resolution,
) -> Result<Bytes, MoqClientError> {
    let sample_entry = mp4_atom::Codec::from(mp4_atom::Vp09 {
        visual: visual(resolution),
        vpcc: mp4_atom::VpcC {
            profile: vp9.profile,
            level: vp9.level,
            bit_depth: vp9.bit_depth,
            chroma_subsampling: vp9.chroma_subsampling,
            video_full_range_flag: vp9.full_range,
            color_primaries: vp9.color_primaries,
            transfer_characteristics: vp9.transfer_characteristics,
            matrix_coefficients: vp9.matrix_coefficients,
            codec_initialization_data: Vec::new(),
        },
        ..Default::default()
    });

    video_cmaf_init_segment(sample_entry, resolution)
}

fn visual(resolution: Resolution) -> mp4_atom::Visual {
    mp4_atom::Visual {
        data_reference_index: 1,
        width: resolution.width as u16,
        height: resolution.height as u16,
        ..Default::default()
    }
}

fn video_cmaf_init_segment(
    sample_entry: mp4_atom::Codec,
    resolution: Resolution,
) -> Result<Bytes, MoqClientError> {
    let trak = mp4_atom::Trak {
        tkhd: mp4_atom::Tkhd {
            track_id: TRACK_ID,
            enabled: true,
            width: mp4_atom::FixedPoint::from(resolution.width as u16),
            height: mp4_atom::FixedPoint::from(resolution.height as u16),
            ..Default::default()
        },
        mdia: mdia(VIDEO_TIMESCALE, b"vide", sample_entry),
        ..Default::default()
    };

    cmaf_init_segment(trak)
}

pub(super) fn opus_cmaf_init_segment(
    sample_rate: u32,
    channels: AudioChannels,
) -> Result<Bytes, MoqClientError> {
    if sample_rate > 48000 {
        return Err(MoqClientError::UnsupportedSampleRate(sample_rate));
    }
    let channel_count = match channels {
        AudioChannels::Mono => 1,
        AudioChannels::Stereo => 2,
    };

    let sample_entry = mp4_atom::Codec::from(mp4_atom::Opus {
        audio: mp4_atom::Audio {
            data_reference_index: 1,
            channel_count: channel_count as u16,
            sample_size: 16,
            sample_rate: mp4_atom::FixedPoint::from(sample_rate as u16),
        },
        // TODO: pre_skip should be the encoder lookahead in 48 kHz samples
        // (~312 for libopus), not 0. The libopus encoder already computes it
        // (`OPUS_GET_LOOKAHEAD`) and stores it in its OpusHead extradata
        // (bytes 10..12, LE), but only the encoder options are plumbed through
        // to here.
        dops: mp4_atom::Dops {
            output_channel_count: channel_count,
            pre_skip: 0,
            input_sample_rate: sample_rate,
            output_gain: 0,
        },
        btrt: None,
    });

    let trak = mp4_atom::Trak {
        tkhd: mp4_atom::Tkhd {
            track_id: TRACK_ID,
            enabled: true,
            volume: mp4_atom::FixedPoint::from(1u8),
            ..Default::default()
        },
        mdia: mdia(sample_rate, b"soun", sample_entry),
        ..Default::default()
    };

    cmaf_init_segment(trak)
}

pub(super) fn aac_cmaf_init_segment(asc: &[u8]) -> Result<Bytes, MoqClientError> {
    let config = AacAudioSpecificConfig::parse_from(asc).map_err(|err| {
        MoqClientError::InitSegmentError(format!("invalid AudioSpecificConfig: {err}"))
    })?;
    let freq_index = sample_rate_to_freq_index(config.sample_rate).ok_or_else(|| {
        MoqClientError::InitSegmentError(format!(
            "unsupported AAC sample rate {}",
            config.sample_rate
        ))
    })?;

    let sample_entry = mp4_atom::Codec::from(mp4_atom::Mp4a {
        audio: mp4_atom::Audio {
            data_reference_index: 1,
            channel_count: config.channel_count as u16,
            sample_size: 16,
            sample_rate: mp4_atom::FixedPoint::from(config.sample_rate as u16),
        },
        esds: mp4_atom::Esds {
            es_desc: mp4_atom::esds::EsDescriptor {
                es_id: TRACK_ID as u16,
                dec_config: mp4_atom::esds::DecoderConfig {
                    // 0x40 = MPEG-4 Audio, 0x05 = AudioStream (MPEG-4 systems tables).
                    object_type_indication: 0x40,
                    stream_type: 0x05,
                    dec_specific: mp4_atom::esds::DecoderSpecific {
                        profile: config.profile,
                        freq_index,
                        chan_conf: config.channel_count,
                    },
                    ..Default::default()
                },
                sl_config: mp4_atom::esds::SLConfig::default(),
            },
        },
        btrt: None,
        taic: None,
    });

    let trak = mp4_atom::Trak {
        tkhd: mp4_atom::Tkhd {
            track_id: TRACK_ID,
            enabled: true,
            volume: mp4_atom::FixedPoint::from(1u8),
            ..Default::default()
        },
        mdia: mdia(config.sample_rate, b"soun", sample_entry),
        ..Default::default()
    };

    cmaf_init_segment(trak)
}

fn cmaf_init_segment(trak: mp4_atom::Trak) -> Result<Bytes, MoqClientError> {
    // CMAF §7.3.2 requires the `cmfc` structural brand in a CMAF header,
    // while `iso6` declares the fragmented-file features used by the segments.
    let ftyp = mp4_atom::Ftyp {
        major_brand: b"iso6".into(),
        minor_version: 0,
        compatible_brands: vec![b"iso6".into(), b"cmfc".into(), b"mp41".into()],
    };

    let moov = mp4_atom::Moov {
        mvhd: mp4_atom::Mvhd {
            timescale: trak.mdia.mdhd.timescale,
            rate: mp4_atom::FixedPoint::from(1u16),
            volume: mp4_atom::FixedPoint::from(1u8),
            ..Default::default()
        },
        mvex: Some(mp4_atom::Mvex {
            trex: vec![mp4_atom::Trex {
                track_id: trak.tkhd.track_id,
                default_sample_description_index: 1,
                ..Default::default()
            }],
            ..Default::default()
        }),
        trak: vec![trak],
        ..Default::default()
    };

    let mut buf = Vec::new();
    ftyp.encode(&mut buf)
        .and_then(|_| moov.encode(&mut buf))
        .map_err(|err| MoqClientError::InitSegmentError(format!("{err}")))?;

    Ok(Bytes::from(buf))
}

fn mdia(timescale: u32, handler: &[u8; 4], sample_entry: mp4_atom::Codec) -> mp4_atom::Mdia {
    let is_video = handler == b"vide";
    mp4_atom::Mdia {
        mdhd: mp4_atom::Mdhd {
            timescale,
            language: "und".to_string(),
            ..Default::default()
        },
        hdlr: mp4_atom::Hdlr {
            handler: mp4_atom::FourCC::new(handler),
            name: String::new(),
        },
        minf: mp4_atom::Minf {
            vmhd: is_video.then(mp4_atom::Vmhd::default),
            smhd: (!is_video).then(mp4_atom::Smhd::default),
            dinf: mp4_atom::Dinf {
                dref: mp4_atom::Dref {
                    urls: vec![mp4_atom::Url::default()],
                },
            },
            stbl: mp4_atom::Stbl {
                stsd: mp4_atom::Stsd {
                    codecs: vec![sample_entry],
                },
                ..Default::default()
            },
            ..Default::default()
        },
    }
}
