//! CMAF init segment (ftyp+moov) construction.
//!
//! moq-mux keeps its own builders private, so we assemble the equivalent atoms
//! here. Only the codecs we allow with the CMAF container are covered: H264 on
//! the video side and Opus on the audio side.
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

pub(super) fn h264(extradata: &[u8], resolution: Resolution) -> Result<Bytes, MoqClientError> {
    let mut cursor = std::io::Cursor::new(extradata);
    let avcc = mp4_atom::Avcc::decode_body(&mut cursor)
        .map_err(|err| MoqClientError::InitSegmentError(format!("invalid avcC record: {err}")))?;

    let width = resolution.width as u16;
    let height = resolution.height as u16;
    let sample_entry = mp4_atom::Codec::from(mp4_atom::Avc1 {
        visual: mp4_atom::Visual {
            data_reference_index: 1,
            width,
            height,
            ..Default::default()
        },
        avcc,
        ..Default::default()
    });

    let trak = mp4_atom::Trak {
        tkhd: mp4_atom::Tkhd {
            track_id: TRACK_ID,
            enabled: true,
            width: mp4_atom::FixedPoint::from(width),
            height: mp4_atom::FixedPoint::from(height),
            ..Default::default()
        },
        mdia: mdia(VIDEO_TIMESCALE, b"vide", sample_entry),
        ..Default::default()
    };

    encode_init(trak)
}

pub(super) fn opus(sample_rate: u32, channels: AudioChannels) -> Result<Bytes, MoqClientError> {
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
            ..Default::default()
        },
        mdia: mdia(sample_rate, b"soun", sample_entry),
        ..Default::default()
    };

    encode_init(trak)
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

fn encode_init(trak: mp4_atom::Trak) -> Result<Bytes, MoqClientError> {
    let ftyp = mp4_atom::Ftyp {
        major_brand: b"isom".into(),
        minor_version: 0x200,
        compatible_brands: vec![b"isom".into(), b"iso6".into(), b"mp41".into()],
    };

    let moov = mp4_atom::Moov {
        mvhd: mp4_atom::Mvhd {
            timescale: trak.mdia.mdhd.timescale,
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
