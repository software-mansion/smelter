//! Minimal MP4 demuxer used by the pipeline-test dump comparison and
//! the audit inspector when a snapshot name ends in `.mp4`.
//!
//! The RTP dump path reads length-prefixed RTP packets; the MP4 path
//! reads stored samples straight out of the container. Both ultimately
//! feed the same H.264 / audio decoders, so once a dump is demuxed
//! here the rest of the comparison machinery is format-agnostic.
//!
//! Only the two tracks smelter's MP4 output produces are handled:
//! H.264 video and AAC audio. Everything else is ignored.

use std::{io::Cursor, time::Duration};

use anyhow::{Context, Result};
use bytes::Bytes;
use mp4::{MediaType, Mp4Reader, TrackType};

/// A single encoded sample taken out of an MP4 track, with its
/// presentation timestamp already converted to a wall-clock duration.
#[derive(Debug, Clone)]
pub struct EncodedSample {
    pub data: Bytes,
    pub pts: Duration,
    /// Whether this sample is a sync sample (IDR for video). Used to
    /// decide where to re-inject SPS/PPS when rebuilding the Annex-B
    /// stream for the decoder.
    pub is_sync: bool,
}

/// H.264 `avcC` configuration needed to rebuild an Annex-B stream from
/// the length-prefixed samples stored in the container.
#[derive(Debug, Clone)]
pub struct H264Config {
    pub nalu_length_size: usize,
    pub sps: Vec<Bytes>,
    pub pps: Vec<Bytes>,
}

pub struct Mp4VideoTrack {
    pub config: H264Config,
    pub samples: Vec<EncodedSample>,
}

pub struct Mp4AudioTrack {
    /// AudioSpecificConfig (the `esds` `DecoderSpecificInfo`). Used to
    /// build ADTS headers so ffmpeg can decode the raw AAC samples.
    pub asc: Bytes,
    pub samples: Vec<EncodedSample>,
}

#[derive(Default)]
pub struct Mp4Dump {
    pub video: Option<Mp4VideoTrack>,
    pub audio: Option<Mp4AudioTrack>,
}

/// Demux an in-memory MP4 file into its H.264 and AAC tracks.
pub fn read_mp4(bytes: &Bytes) -> Result<Mp4Dump> {
    let size = bytes.len() as u64;
    let mut reader = Mp4Reader::read_header(Cursor::new(bytes.clone()), size)
        .context("Failed to read MP4 header")?;

    let video_meta = find_h264_track(&reader);
    let audio_meta = find_aac_track(&reader);

    let mut dump = Mp4Dump::default();

    if let Some((track_id, timescale, config)) = video_meta {
        let samples = read_samples(&mut reader, track_id, timescale, true)?;
        dump.video = Some(Mp4VideoTrack { config, samples });
    }
    if let Some((track_id, timescale, asc)) = audio_meta {
        let samples = read_samples(&mut reader, track_id, timescale, false)?;
        dump.audio = Some(Mp4AudioTrack { asc, samples });
    }

    Ok(dump)
}

type VideoMeta = (u32, u32, H264Config);
type AudioMeta = (u32, u32, Bytes);

fn find_h264_track(reader: &Mp4Reader<Cursor<Bytes>>) -> Option<VideoMeta> {
    let (&track_id, track, avc) = reader.tracks().iter().find_map(|(id, track)| {
        let track_type = track.track_type().ok()?;
        let media_type = track.media_type().ok()?;
        let avc = track.avc1_or_3_inner();
        if track_type != TrackType::Video || media_type != MediaType::H264 || avc.is_none() {
            return None;
        }
        avc.map(|avc| (id, track, avc))
    })?;

    let config = H264Config {
        nalu_length_size: (avc.avcc.length_size_minus_one & 0x3) as usize + 1,
        sps: avc
            .avcc
            .sequence_parameter_sets
            .iter()
            .map(|nalu| Bytes::copy_from_slice(&nalu.bytes))
            .collect(),
        pps: avc
            .avcc
            .picture_parameter_sets
            .iter()
            .map(|nalu| Bytes::copy_from_slice(&nalu.bytes))
            .collect(),
    };
    Some((track_id, track.timescale(), config))
}

fn find_aac_track(reader: &Mp4Reader<Cursor<Bytes>>) -> Option<AudioMeta> {
    let (&track_id, track, aac) = reader.tracks().iter().find_map(|(id, track)| {
        let track_type = track.track_type().ok()?;
        let media_type = track.media_type().ok()?;
        let aac = track.trak.mdia.minf.stbl.stsd.mp4a.as_ref();
        if track_type != TrackType::Audio || media_type != MediaType::AAC || aac.is_none() {
            return None;
        }
        aac.map(|aac| (id, track, aac))
    })?;

    let asc = aac
        .esds
        .as_ref()
        .and_then(|esds| esds.es_desc.dec_config.dec_specific.full_config.clone())
        .map(Bytes::from)?;
    Some((track_id, track.timescale(), asc))
}

/// Read every sample of a track, converting timestamps to durations.
/// `with_rendering_offset` shifts the pts by the sample's composition
/// offset (B-frame reordering) for video; audio has no such offset.
fn read_samples(
    reader: &mut Mp4Reader<Cursor<Bytes>>,
    track_id: u32,
    timescale: u32,
    with_rendering_offset: bool,
) -> Result<Vec<EncodedSample>> {
    let sample_count = reader
        .sample_count(track_id)
        .with_context(|| format!("Failed to read sample count for track {track_id}"))?;

    let mut samples = Vec::with_capacity(sample_count as usize);
    for sample_id in 1..=sample_count {
        let Some(sample) = reader
            .read_sample(track_id, sample_id)
            .with_context(|| format!("Failed to read sample {sample_id} of track {track_id}"))?
        else {
            continue;
        };
        let offset = if with_rendering_offset {
            sample.rendering_offset as i64
        } else {
            0
        };
        let ticks = (sample.start_time as i64 + offset).max(0) as f64;
        samples.push(EncodedSample {
            data: sample.bytes,
            pts: Duration::from_secs_f64(ticks / timescale as f64),
            is_sync: sample.is_sync,
        });
    }
    Ok(samples)
}

/// Rebuild an Annex-B encoded access unit from a length-prefixed
/// (`avcC`) MP4 sample. SPS/PPS are re-injected ahead of every sync
/// sample so the decoder can start from any keyframe.
pub fn sample_to_annex_b(sample: &EncodedSample, config: &H264Config) -> Bytes {
    const START_CODE: [u8; 4] = [0, 0, 0, 1];
    let mut out = Vec::with_capacity(sample.data.len() + 64);

    if sample.is_sync {
        for nalu in config.sps.iter().chain(config.pps.iter()) {
            out.extend_from_slice(&START_CODE);
            out.extend_from_slice(nalu);
        }
    }

    let data = &sample.data;
    let len_size = config.nalu_length_size;
    let mut pos = 0;
    while pos + len_size <= data.len() {
        let mut nalu_len = 0usize;
        for &b in &data[pos..pos + len_size] {
            nalu_len = (nalu_len << 8) | b as usize;
        }
        pos += len_size;
        if pos + nalu_len > data.len() {
            break;
        }
        out.extend_from_slice(&START_CODE);
        out.extend_from_slice(&data[pos..pos + nalu_len]);
        pos += nalu_len;
    }
    Bytes::from(out)
}
