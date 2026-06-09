//! Format-aware decoding of pipeline-test output dumps.
//!
//! A dump is either a length-prefixed RTP packet stream (`.rtp`) or an
//! MP4 file (`.mp4`). The snapshot filename's extension selects which
//! demuxer/decoder pair to use; everything downstream (frame pairing,
//! MSE/FFT comparison, the audit inspector) works on the decoded
//! [`Frame`] / [`AudioSampleBatch`] streams and doesn't care which.

use std::path::Path;

use anyhow::{Result, bail};
use bytes::Bytes;
use smelter_render::Frame;

use crate::{
    aac_decoder::AacDecoder,
    audio_decoder::{AudioChannels, AudioDecoder, AudioSampleBatch},
    find_packets_for_payload_type,
    mp4_reader::{read_mp4, sample_to_annex_b},
    unmarshal_packets,
    video_decoder::VideoDecoder,
};

/// RTP payload type smelter uses for H.264 video.
pub const VIDEO_PAYLOAD_TYPE: u8 = 96;
/// RTP payload type smelter uses for OPUS audio.
pub const AUDIO_PAYLOAD_TYPE: u8 = 97;
/// Sample rate used to decode and analyse audio in this crate.
pub const AUDIO_SAMPLE_RATE: u32 = 48_000;

/// Container format of an output dump, derived from its snapshot name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DumpFormat {
    Rtp,
    Mp4,
}

impl DumpFormat {
    /// Pick the format from a snapshot filename: `*.rtp` -> [`Self::Rtp`],
    /// `*.mp4` -> [`Self::Mp4`]. Anything else is an error so a typo'd
    /// snapshot name fails loudly rather than silently picking a format.
    pub fn from_path<P: AsRef<Path>>(path: P) -> Result<Self> {
        match path.as_ref().extension().and_then(|e| e.to_str()) {
            Some("rtp") => Ok(Self::Rtp),
            Some("mp4") => Ok(Self::Mp4),
            other => bail!(
                "cannot determine dump format from snapshot name {:?} \
                 (expected an .rtp or .mp4 extension, got {:?})",
                path.as_ref(),
                other
            ),
        }
    }
}

/// Which media kinds a dump actually contains. Drives the audit
/// inspector's Video / Audio prompt.
#[derive(Debug, Clone, Copy, Default)]
pub struct MediaKinds {
    pub video: bool,
    pub audio: bool,
}

/// Inspect a dump and report which media kinds it carries.
pub fn dump_media_kinds(bytes: &Bytes, format: DumpFormat) -> Result<MediaKinds> {
    match format {
        DumpFormat::Rtp => {
            let mut kinds = MediaKinds::default();
            for packet in unmarshal_packets(bytes)? {
                match packet.header.payload_type {
                    VIDEO_PAYLOAD_TYPE => kinds.video = true,
                    AUDIO_PAYLOAD_TYPE => kinds.audio = true,
                    _ => {}
                }
            }
            Ok(kinds)
        }
        DumpFormat::Mp4 => {
            let dump = read_mp4(bytes)?;
            Ok(MediaKinds {
                video: dump.video.is_some(),
                audio: dump.audio.is_some(),
            })
        }
    }
}

/// Decode every video frame in a dump. Frames are returned sorted by
/// presentation timestamp, matching the ordering the frame-pairing
/// iterator expects.
pub fn decode_video_dump(bytes: &Bytes, format: DumpFormat) -> Result<Vec<Frame>> {
    let mut frames = match format {
        DumpFormat::Rtp => decode_rtp_video(bytes)?,
        DumpFormat::Mp4 => decode_mp4_video(bytes)?,
    };
    frames.sort_by_key(|f| f.pts);
    Ok(frames)
}

/// Decode every audio sample batch in a dump.
pub fn decode_audio_dump(bytes: &Bytes, format: DumpFormat) -> Result<Vec<AudioSampleBatch>> {
    match format {
        DumpFormat::Rtp => decode_rtp_audio(bytes),
        DumpFormat::Mp4 => decode_mp4_audio(bytes),
    }
}

fn decode_rtp_video(bytes: &Bytes) -> Result<Vec<Frame>> {
    let packets = unmarshal_packets(bytes)?;
    let packets = find_packets_for_payload_type(&packets, VIDEO_PAYLOAD_TYPE);
    let mut decoder = VideoDecoder::new()?;
    for packet in packets {
        decoder.decode(packet)?;
    }
    decoder.drain_frames()
}

fn decode_mp4_video(bytes: &Bytes) -> Result<Vec<Frame>> {
    let dump = read_mp4(bytes)?;
    let Some(track) = dump.video else {
        return Ok(Vec::new());
    };
    let mut decoder = VideoDecoder::new()?;
    let mut frames = Vec::new();
    for sample in &track.samples {
        let annex_b = sample_to_annex_b(sample, &track.config);
        decoder.decode_annex_b(&annex_b, sample.pts)?;
        frames.append(&mut decoder.drain_frames()?);
    }
    frames.append(&mut decoder.flush()?);
    Ok(frames)
}

fn decode_rtp_audio(bytes: &Bytes) -> Result<Vec<AudioSampleBatch>> {
    let packets = unmarshal_packets(bytes)?;
    let packets = find_packets_for_payload_type(&packets, AUDIO_PAYLOAD_TYPE);
    let mut decoder = AudioDecoder::new(AUDIO_SAMPLE_RATE, AudioChannels::Stereo)?;
    for packet in packets {
        decoder.decode(packet)?;
    }
    Ok(decoder.take_samples())
}

fn decode_mp4_audio(bytes: &Bytes) -> Result<Vec<AudioSampleBatch>> {
    let dump = read_mp4(bytes)?;
    let Some(track) = dump.audio else {
        return Ok(Vec::new());
    };
    let mut decoder = AacDecoder::new(&track.asc)?;
    for sample in &track.samples {
        decoder.decode(&sample.data, sample.pts)?;
    }
    decoder.take_samples()
}

/// Read a dump from disk, treating a missing file as empty so the
/// inspector can still surface whichever side does exist.
pub fn decode_video_dump_path(path: &Path, format: DumpFormat) -> Result<Vec<Frame>> {
    let Some(bytes) = read_optional(path)? else {
        return Ok(Vec::new());
    };
    decode_video_dump(&bytes, format)
}

/// Same as [`decode_video_dump_path`] for audio.
pub fn decode_audio_dump_path(path: &Path, format: DumpFormat) -> Result<Vec<AudioSampleBatch>> {
    let Some(bytes) = read_optional(path)? else {
        return Ok(Vec::new());
    };
    decode_audio_dump(&bytes, format)
}

fn read_optional(path: &Path) -> Result<Option<Bytes>> {
    if !path.exists() {
        tracing::warn!(
            "media_dump: dump {} not found, treating as empty",
            path.display()
        );
        return Ok(None);
    }
    Ok(Some(Bytes::from(std::fs::read(path)?)))
}
