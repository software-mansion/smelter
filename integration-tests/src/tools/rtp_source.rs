//! Frame and sample sources for length-prefixed RTP packet dumps,
//! used by the pipeline-test harness and the dump inspector.
//! Counterpart to [`super::mp4_source`].
//!
//! Video frames are decoded lazily through [`RtpVideoFrameSource`] so
//! callers never hold more than a handful of decoded frames at once;
//! audio is small enough to decode eagerly via [`decode_opus_audio`].

use std::{collections::HashSet, path::Path};

use anyhow::{Context, Result};
use bytes::Bytes;
use smelter_render::Frame;
use strum::{Display, EnumIter};
use tracing::warn;
use webrtc::rtp;

use crate::{
    DumpFormat,
    audio_decoder::{AudioChannels, AudioDecoder, AudioSampleBatch},
    tools::{mp4_source, video_diff_iter::LazyFrameSource},
    unmarshal_packets,
    video_decoder::VideoDecoder,
};

/// RTP payload type smelter uses for H.264 video.
pub(crate) const VIDEO_PAYLOAD_TYPE: u8 = 96;
/// RTP payload type smelter uses for OPUS audio.
pub(crate) const AUDIO_PAYLOAD_TYPE: u8 = 97;

#[derive(Debug, Clone, Copy, Display, EnumIter)]
pub enum MediaKind {
    #[strum(to_string = "Video")]
    Video,
    #[strum(to_string = "Audio")]
    Audio,
}

/// Inspect each dump once to see which media kinds it contains, used
/// to gate a Video / Audio prompt. Missing files are skipped with a
/// warning so an inspector can still launch when one side (typically
/// the committed `expected` snapshot) doesn't exist.
pub fn available_media_kinds(
    format: DumpFormat,
    paths: &[&Path],
) -> Result<Vec<MediaKind>> {
    let mut has_video = false;
    let mut has_audio = false;
    for path in paths {
        if !path.exists() {
            warn!("rtp_source: dump {} not found, skipping", path.display());
            continue;
        }
        let bytes = Bytes::from(
            std::fs::read(path)
                .with_context(|| format!("Failed to read {}", path.display()))?,
        );
        match format {
            DumpFormat::Rtp => {
                let types = scan_payload_types(&bytes).with_context(|| {
                    format!("Failed to parse RTP dump {}", path.display())
                })?;
                has_video |= types.contains(&VIDEO_PAYLOAD_TYPE);
                has_audio |= types.contains(&AUDIO_PAYLOAD_TYPE);
            }
            DumpFormat::Mp4 => {
                let streams = mp4_source::probe_streams(&bytes).with_context(|| {
                    format!("Failed to probe MP4 dump {}", path.display())
                })?;
                has_video |= streams.has_video;
                has_audio |= streams.has_audio;
            }
        }
    }
    let mut kinds = Vec::new();
    if has_video {
        kinds.push(MediaKind::Video);
    }
    if has_audio {
        kinds.push(MediaKind::Audio);
    }
    Ok(kinds)
}

/// Collect the set of RTP payload types present in a dump.
fn scan_payload_types(bytes: &Bytes) -> Result<HashSet<u8>> {
    let mut types = HashSet::new();
    for packet in unmarshal_packets(bytes)? {
        types.insert(packet.header.payload_type);
    }
    Ok(types)
}

/// Lazy frame source over the H.264 video packets of an RTP packet
/// dump. Holds the (still encoded) RTP packets in memory and pumps
/// them through [`VideoDecoder`] one at a time as
/// [`LazyFrameSource::next_batch`] is called.
pub(crate) struct RtpVideoFrameSource {
    decoder: VideoDecoder,
    packets: std::vec::IntoIter<rtp::packet::Packet>,
    flushed: bool,
}

impl RtpVideoFrameSource {
    pub(crate) fn from_bytes(dump: &Bytes) -> Result<Self> {
        let packets = unmarshal_packets(dump)
            .context("Failed to parse RTP dump")?
            .into_iter()
            .filter(|p| p.header.payload_type == VIDEO_PAYLOAD_TYPE)
            .collect::<Vec<_>>();
        let decoder =
            VideoDecoder::new().context("Failed to initialize H.264 decoder")?;
        Ok(Self { decoder, packets: packets.into_iter(), flushed: false })
    }
}

impl LazyFrameSource for RtpVideoFrameSource {
    fn next_batch(&mut self) -> Result<Option<Vec<Frame>>> {
        match self.packets.next() {
            Some(packet) => {
                self.decoder.decode(packet)?;
                Ok(Some(self.decoder.drain_frames()?))
            }
            // No more input packets: pull whatever the decoder has
            // buffered, then report drained.
            None if !self.flushed => {
                self.flushed = true;
                Ok(Some(self.decoder.drain_frames()?))
            }
            None => Ok(None),
        }
    }
}

/// Decode the whole OPUS audio track of an RTP packet dump. Audio is
/// small, so unlike video there is no lazy variant.
///
/// Each decoder output chunk keeps its original presentation
/// timestamp; chunks are intentionally not flattened so callers like
/// the waveform inspector can show per-chunk boundaries.
pub fn decode_opus_audio(
    dump: &Bytes,
    sample_rate: u32,
) -> Result<Vec<AudioSampleBatch>> {
    let packets = unmarshal_packets(dump)
        .context("Failed to parse RTP dump")?
        .into_iter()
        .filter(|p| p.header.payload_type == AUDIO_PAYLOAD_TYPE);
    let mut decoder = AudioDecoder::new(sample_rate, AudioChannels::Stereo)
        .context("Failed to initialize OPUS decoder")?;
    for packet in packets {
        decoder.decode(packet).context("Failed to decode audio packet")?;
    }
    Ok(decoder.take_samples())
}
