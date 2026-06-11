//! Frame and sample sources for MP4 output dumps, used by the
//! pipeline-test harness and the dump inspector. Counterpart to
//! [`super::rtp_source`].
//!
//! An in-memory MP4 dump is demuxed up front into per-stream lists of
//! encoded packets (the encoded data stays in memory; nothing is
//! decoded yet). Video frames are then decoded lazily through
//! [`Mp4VideoFrameSource`] so callers never hold more than a handful
//! of decoded frames at once; audio is small enough to decode eagerly
//! via [`decode_aac_audio`].
//!
//! ffmpeg can only open inputs by path, so demuxing round-trips the
//! bytes through a temporary file that is removed as soon as the
//! packets are read.

use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

use anyhow::{Context as _, Result, bail};
use bytes::Bytes;
use ffmpeg_next::{
    Packet, Rational,
    codec::Context as FfmpegContext,
    decoder, format,
    format::{Pixel, Sample, sample},
    frame,
    media::Type,
};
use smelter_render::{Frame, FrameData, Resolution, YuvPlanes};

use crate::{
    audio_decoder::AudioSampleBatch, tools::video_diff_iter::LazyFrameSource,
    video_decoder::copy_plane_from_av,
};

/// Which media streams an MP4 dump contains. Used by the dump
/// inspector to gate its Video / Audio prompt.
pub(crate) struct Mp4Streams {
    pub(crate) has_video: bool,
    pub(crate) has_audio: bool,
}

/// Open the MP4 just far enough to see which streams exist; nothing
/// is demuxed or decoded.
pub(crate) fn probe_streams(dump: &Bytes) -> Result<Mp4Streams> {
    let path = write_temp_file(dump)?;
    let result = (|| {
        let input = format::input(&path).context("Failed to open MP4 dump with ffmpeg")?;
        Ok(Mp4Streams {
            has_video: input.streams().best(Type::Video).is_some(),
            has_audio: input.streams().best(Type::Audio).is_some(),
        })
    })();
    let _ = std::fs::remove_file(&path);
    result
}

struct DemuxedStream {
    parameters: ffmpeg_next::codec::Parameters,
    time_base: Rational,
    packets: Vec<Packet>,
}

struct DemuxedMp4 {
    video: Option<DemuxedStream>,
    audio: Option<DemuxedStream>,
}

fn demux(dump: &Bytes) -> Result<DemuxedMp4> {
    let path = write_temp_file(dump)?;
    let result = demux_path(&path);
    let _ = std::fs::remove_file(&path);
    result
}

fn demux_path(path: &Path) -> Result<DemuxedMp4> {
    let mut input = format::input(path).context("Failed to open MP4 dump with ffmpeg")?;
    let stream_meta = |stream: ffmpeg_next::Stream| {
        (
            stream.index(),
            DemuxedStream {
                parameters: stream.parameters(),
                time_base: stream.time_base(),
                packets: Vec::new(),
            },
        )
    };
    let mut video = input.streams().best(Type::Video).map(stream_meta);
    let mut audio = input.streams().best(Type::Audio).map(stream_meta);
    for (stream, packet) in input.packets() {
        if let Some((index, demuxed)) = video.as_mut()
            && stream.index() == *index
        {
            demuxed.packets.push(packet);
        } else if let Some((index, demuxed)) = audio.as_mut()
            && stream.index() == *index
        {
            demuxed.packets.push(packet);
        }
    }
    Ok(DemuxedMp4 {
        video: video.map(|(_, demuxed)| demuxed),
        audio: audio.map(|(_, demuxed)| demuxed),
    })
}

static TEMP_FILE_ID: AtomicU64 = AtomicU64::new(0);

fn write_temp_file(bytes: &Bytes) -> Result<PathBuf> {
    let path = std::env::temp_dir().join(format!(
        "smelter-test-mp4-dump-{}-{}.mp4",
        std::process::id(),
        TEMP_FILE_ID.fetch_add(1, Ordering::Relaxed),
    ));
    std::fs::write(&path, bytes)
        .with_context(|| format!("Failed to write temporary MP4 dump {}", path.display()))?;
    Ok(path)
}

/// Lazy frame source over the video track of an MP4 dump. Holds the
/// demuxed (still encoded) packets in memory and decodes them one at
/// a time as [`LazyFrameSource::next_batch`] is called.
pub(crate) struct Mp4VideoFrameSource {
    decoder: decoder::Video,
    packets: std::vec::IntoIter<Packet>,
    time_base: Rational,
    first_pts: Option<Duration>,
    flushed: bool,
}

impl Mp4VideoFrameSource {
    pub(crate) fn from_bytes(dump: &Bytes) -> Result<Self> {
        let Some(stream) = demux(dump)?.video else {
            bail!("MP4 dump has no video stream");
        };
        let decoder = FfmpegContext::from_parameters(stream.parameters)?
            .decoder()
            .video()
            .context("Failed to initialize video decoder for MP4 dump")?;
        Ok(Self {
            decoder,
            packets: stream.packets.into_iter(),
            time_base: stream.time_base,
            first_pts: None,
            flushed: false,
        })
    }

    fn receive_frames(&mut self) -> Result<Vec<Frame>> {
        let mut frames = Vec::new();
        let mut decoded = frame::Video::empty();
        while self.decoder.receive_frame(&mut decoded).is_ok() {
            frames.push(self.convert_frame(&decoded)?);
        }
        Ok(frames)
    }

    fn convert_frame(&mut self, decoded: &frame::Video) -> Result<Frame> {
        let planes = YuvPlanes {
            y_plane: copy_plane_from_av(decoded, 0),
            u_plane: copy_plane_from_av(decoded, 1),
            v_plane: copy_plane_from_av(decoded, 2),
        };
        let data = match decoded.format() {
            Pixel::YUV420P => FrameData::PlanarYuv420(planes),
            Pixel::YUVJ420P => FrameData::PlanarYuvJ420(planes),
            other => bail!("unsupported pixel format {other:?} in MP4 dump"),
        };
        let resolution = Resolution {
            width: decoded.width().try_into()?,
            height: decoded.height().try_into()?,
        };
        let pts = pts_to_duration(decoded.pts().or(decoded.timestamp()), self.time_base)?;
        // Decoded frames come out in presentation order, so the first
        // one is the earliest; normalize the timeline to start at zero
        // like the RTP decode path does.
        let first_pts = *self.first_pts.get_or_insert(pts);
        let pts = pts
            .checked_sub(first_pts)
            .context("MP4 video frame pts earlier than the first frame")?;
        Ok(Frame {
            data,
            resolution,
            pts,
        })
    }
}

impl LazyFrameSource for Mp4VideoFrameSource {
    fn next_batch(&mut self) -> Result<Option<Vec<Frame>>> {
        match self.packets.next() {
            Some(packet) => {
                self.decoder.send_packet(&packet)?;
                Ok(Some(self.receive_frames()?))
            }
            // No more input packets: flush the decoder and pull
            // whatever it has buffered, then report drained.
            None if !self.flushed => {
                self.flushed = true;
                self.decoder.send_eof()?;
                Ok(Some(self.receive_frames()?))
            }
            None => Ok(None),
        }
    }
}

/// Decode the whole AAC audio track of an MP4 dump. Audio is small,
/// so unlike video there is no lazy variant.
///
/// `expected_sample_rate` is the rate the downstream analysis runs
/// at; a track encoded at any other rate is rejected rather than
/// silently producing misaligned sample indices.
pub fn decode_aac_audio(dump: &Bytes, expected_sample_rate: u32) -> Result<Vec<AudioSampleBatch>> {
    let Some(stream) = demux(dump)?.audio else {
        bail!("MP4 dump has no audio stream");
    };
    let mut decoder = FfmpegContext::from_parameters(stream.parameters)?
        .decoder()
        .audio()
        .context("Failed to initialize AAC decoder for MP4 dump")?;

    let mut batches = Vec::new();
    let mut first_pts: Option<Duration> = None;
    let mut decoded = frame::Audio::empty();
    let mut receive_all = |decoder: &mut decoder::Audio,
                           batches: &mut Vec<AudioSampleBatch>,
                           first_pts: &mut Option<Duration>|
     -> Result<()> {
        while decoder.receive_frame(&mut decoded).is_ok() {
            batches.push(convert_audio_frame(
                &decoded,
                stream.time_base,
                first_pts,
                expected_sample_rate,
            )?);
        }
        Ok(())
    };

    for packet in &stream.packets {
        decoder.send_packet(packet)?;
        receive_all(&mut decoder, &mut batches, &mut first_pts)?;
    }
    decoder.send_eof()?;
    receive_all(&mut decoder, &mut batches, &mut first_pts)?;
    Ok(batches)
}

fn convert_audio_frame(
    decoded: &frame::Audio,
    time_base: Rational,
    first_pts: &mut Option<Duration>,
    expected_sample_rate: u32,
) -> Result<AudioSampleBatch> {
    if decoded.rate() != expected_sample_rate {
        bail!(
            "MP4 audio sample rate {} does not match the {expected_sample_rate} Hz the audio \
             analysis assumes — register the AAC encoder with `sample_rate: {expected_sample_rate}`",
            decoded.rate()
        );
    }
    if decoded.channels() != 2 {
        bail!(
            "expected stereo audio in MP4 dump, got {} channel(s)",
            decoded.channels()
        );
    }
    if decoded.format() != Sample::F32(sample::Type::Planar) {
        bail!(
            "unsupported audio sample format {:?} in MP4 dump",
            decoded.format()
        );
    }
    let left = decoded.plane::<f32>(0);
    let right = decoded.plane::<f32>(1);
    // Interleave and scale to the i16 range the OPUS decode path
    // produces, so downstream thresholds behave the same for both
    // formats.
    let mut samples = Vec::with_capacity(left.len() * 2);
    for (l, r) in left.iter().zip(right.iter()) {
        samples.push(l * 32767.0);
        samples.push(r * 32767.0);
    }
    let pts = pts_to_duration(decoded.pts().or(decoded.timestamp()), time_base)?;
    let first = *first_pts.get_or_insert(pts);
    let pts = pts
        .checked_sub(first)
        .context("MP4 audio frame pts earlier than the first frame")?;
    Ok(AudioSampleBatch { samples, pts })
}

fn pts_to_duration(pts: Option<i64>, time_base: Rational) -> Result<Duration> {
    let pts = pts.context("missing pts")?;
    if pts < 0 {
        bail!("negative pts");
    }
    Ok(Duration::from_secs_f64(pts as f64 * f64::from(time_base)))
}
