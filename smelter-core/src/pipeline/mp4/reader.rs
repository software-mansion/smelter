use std::{
    fs::File,
    io::{Read, Seek},
    os::unix::fs::MetadataExt,
    path::Path,
    time::Duration,
};

use bytes::Bytes;
use mp4::{Mp4Sample, Mp4Track};
use tracing::warn;

use crate::pipeline::utils::H264AvcDecoderConfig;

use crate::prelude::*;

pub(super) struct Mp4FileReader<Reader: Read + Seek + Send + 'static> {
    reader: mp4::Mp4Reader<Reader>,
}

#[derive(Debug, Clone)]
pub(super) enum DecoderOptions {
    H264(H264AvcDecoderConfig),
    Aac(Bytes),
}

impl Mp4FileReader<File> {
    pub fn from_path(path: &Path) -> Result<Self, Mp4InputError> {
        let file = std::fs::File::open(path)?;
        let size = file.metadata()?.size();
        Self::new(file, size)
    }
}

impl<Reader: Read + Seek + Send + 'static> Mp4FileReader<Reader> {
    fn new(reader: Reader, size: u64) -> Result<Self, Mp4InputError> {
        let reader = mp4::Mp4Reader::read_header(reader, size)?;

        Ok(Mp4FileReader { reader })
    }

    pub fn try_new_aac_track(self) -> Option<Track<Reader>> {
        let (&track_id, track, aac) = self.reader.tracks().iter().find_map(|(id, track)| {
            let track_type = track.track_type().ok()?;
            let media_type = track.media_type().ok()?;
            let aac = track.trak.mdia.minf.stbl.stsd.mp4a.as_ref();

            if track_type != mp4::TrackType::Audio
                || media_type != mp4::MediaType::AAC
                || aac.is_none()
            {
                return None;
            }

            aac.map(|aac| (id, track, aac))
        })?;

        let asc = aac
            .esds
            .as_ref()
            .and_then(|esds| esds.es_desc.dec_config.dec_specific.full_config.clone())
            .map(Bytes::from);
        let Some(asc) = asc else {
            warn!("Decoder options for AAC track were not found.");
            return None;
        };

        let (offset, delay) = Self::calculate_elst_edits(track, self.reader.timescale());

        Some(Track {
            sample_count: track.sample_count(),
            timescale: track.timescale(),
            track_id,
            duration: track.duration(),
            decoder_options: DecoderOptions::Aac(asc),
            offset,
            delay,
            reader: self.reader,
        })
    }

    pub fn try_new_h264_track(self) -> Option<Track<Reader>> {
        let (&track_id, track, avc) = self.reader.tracks().iter().find_map(|(id, track)| {
            let track_type = track.track_type().ok()?;
            let media_type = track.media_type().ok()?;
            let avc = track.avc1_or_3_inner();

            if track_type != mp4::TrackType::Video
                || media_type != mp4::MediaType::H264
                || avc.is_none()
            {
                return None;
            }

            avc.map(|avc| (id, track, avc))
        })?;

        let h264_config = H264AvcDecoderConfig {
            nalu_length_size: avc.avcc.length_size_minus_one as usize + 1,
            spss: avc
                .avcc
                .sequence_parameter_sets
                .iter()
                .map(|nalu| Bytes::copy_from_slice(&nalu.bytes))
                .collect(),
            ppss: avc
                .avcc
                .picture_parameter_sets
                .iter()
                .map(|nalu| Bytes::copy_from_slice(&nalu.bytes))
                .collect(),
        };

        let (offset, delay) = Self::calculate_elst_edits(track, self.reader.timescale());

        Some(Track {
            sample_count: track.sample_count(),
            timescale: track.timescale(),
            track_id,
            duration: track.duration(),
            decoder_options: DecoderOptions::H264(h264_config),
            offset,
            delay,
            reader: self.reader,
        })
    }

    /// Calculates the media-time offset and presentation delay from the edit list.
    ///
    /// Returns `(offset, delay)` where:
    /// - `offset` is derived from `media_time` of the first non-empty edit. Contains information
    ///   how much time should be cut from the beginning of the track.
    /// - `delay` is the sum of `duration` of all leading empty edits. Contains information on how
    ///   much track presentation should be delayed
    fn calculate_elst_edits(track: &Mp4Track, movie_timescale: u32) -> (Duration, Duration) {
        let entries = track
            .trak
            .edts
            .as_ref()
            .and_then(|edts| edts.elst.as_ref())
            .map(|elst| elst.entries.as_slice())
            .unwrap_or(&[]);

        let mut delay_ticks: u64 = 0;
        let mut offset = Duration::ZERO;

        for entry in entries {
            // u32::MAX value is the result of overflowing -1
            if entry.media_time == u32::MAX as u64 || entry.media_time == u64::MAX {
                delay_ticks += entry.segment_duration;
            } else {
                offset =
                    Duration::from_secs_f64(entry.media_time as f64 / track.timescale() as f64);
                break;
            }
        }

        let delay = Duration::from_secs_f64(delay_ticks as f64 / movie_timescale as f64);
        (offset, delay)
    }
}

pub(crate) struct Track<Reader: Read + Seek + Send + 'static> {
    reader: mp4::Mp4Reader<Reader>,
    sample_count: u32,
    timescale: u32,
    track_id: u32,
    duration: Duration,
    decoder_options: DecoderOptions,
    offset: Duration,
    delay: Duration,
}

impl<Reader: Read + Seek + Send + 'static> Track<Reader> {
    pub(crate) fn chunks(&mut self, seek: Option<Duration>) -> TrackChunks<'_, Reader> {
        let seek = match seek {
            Some(seek) => seek + self.offset,
            None => self.offset,
        };

        match self.find_seek_start_sample(seek) {
            Some((start_index, present_index)) => TrackChunks {
                track: self,
                seek,
                next_sample_index: start_index,
                present_from_index: present_index,
            },
            None => TrackChunks {
                track: self,
                seek: Duration::ZERO,
                next_sample_index: 1,
                present_from_index: 1,
            },
        }
    }

    pub(super) fn decoder_options(&self) -> &DecoderOptions {
        &self.decoder_options
    }

    pub(super) fn duration(&self) -> Option<Duration> {
        if self.duration == Duration::ZERO {
            None
        } else {
            Some(self.duration)
        }
    }

    /// Returns `(start_index, present_from_index)` for the given seek position.
    /// `start_index` is the last sync sample before seek (for decoder warmup).
    /// `present_from_index` is the first sample at or after seek.
    /// Returns `None` if seek is past the end.
    fn find_seek_start_sample(&self, seek: Duration) -> Option<(u32, u32)> {
        let seek_timestamp = (seek.as_secs_f64() * self.timescale as f64) as u64;
        let track = &self.reader.tracks()[&self.track_id];

        // The STTS box maps samples to batches of samples with the same length
        let stts = &track.trak.mdia.minf.stbl.stts;

        let mut samples_skipped = 0u32;
        let mut skipped_duration = 0u64;
        let mut present_from_index = None;

        // Finds the first sample after the provided seek point.
        for entry in &stts.entries {
            let batch_duration = entry.sample_count as u64 * entry.sample_delta as u64;
            let duration_remaining = seek_timestamp - skipped_duration;

            if duration_remaining < batch_duration {
                let samples_remaining =
                    duration_remaining.div_ceil(entry.sample_delta as u64) as u32;

                present_from_index = Some(samples_remaining + samples_skipped + 1);
                break;
            }

            skipped_duration += batch_duration;
            samples_skipped += entry.sample_count;
        }

        let present_from_index = u32::max(present_from_index?, 1);

        // The STSS box contains indices of sync samples (e.g. key frames).
        // `None` means all samples are sync samples.
        let stss = &track.trak.mdia.minf.stbl.stss;

        let sync_index = match &stss {
            Some(stss) => {
                let pos = stss.entries.partition_point(|&s| s <= present_from_index);

                match pos {
                    0 => 1, // No sync sample found, fall back to the first sample
                    _ => *stss.entries.get(pos - 1).unwrap_or(&1),
                }
            }
            None => present_from_index,
        };

        Some((sync_index, present_from_index))
    }
}

pub(crate) struct TrackChunks<'a, Reader: Read + Seek + Send + 'static> {
    track: &'a mut Track<Reader>,
    seek: Duration,
    next_sample_index: u32,
    present_from_index: u32,
}

impl<Reader: Read + Seek + Send + 'static> Iterator for TrackChunks<'_, Reader> {
    type Item = (EncodedInputChunk, Duration);

    fn next(&mut self) -> Option<Self::Item> {
        while self.next_sample_index <= self.track.sample_count {
            let sample_index = self.next_sample_index;
            let sample = self
                .track
                .reader
                .read_sample(self.track.track_id, sample_index);
            self.next_sample_index += 1;
            match sample {
                Ok(Some(sample)) => {
                    return Some(self.sample_into_chunk(sample, sample_index));
                }
                Ok(None) => {}
                Err(err) => {
                    warn!("Error while reading MP4 sample: {:?}", err);
                }
            };
        }
        None
    }
}

impl<Reader: Read + Seek + Send + 'static> TrackChunks<'_, Reader> {
    fn sample_into_chunk(
        &mut self,
        sample: Mp4Sample,
        sample_index: u32,
    ) -> (EncodedInputChunk, Duration) {
        let rendering_offset = sample.rendering_offset;
        let start_time = sample.start_time;
        let sample_duration =
            Duration::from_secs_f64(sample.duration as f64 / self.track.timescale as f64);
        let delay = self.track.delay;
        tracing::error!(?delay);

        let dts = Duration::from_secs_f64(start_time as f64 / self.track.timescale as f64);
        let mut pts = Duration::from_secs_f64(
            (start_time as f64 + rendering_offset as f64) / self.track.timescale as f64,
        );
        pts += delay;
        pts = pts.saturating_sub(self.seek);

        // When seeking in video, we start reading from the nearest sync (keyframe)
        // sample before the seek point so the decoder can build up its reference
        // frames. Samples before `present_from_sample` are only needed for decoding
        // and should not be presented.
        let present = sample_index >= self.present_from_index;

        let chunk = EncodedInputChunk {
            data: sample.bytes,
            pts,
            dts: Some(dts),
            kind: match self.track.decoder_options {
                DecoderOptions::H264(_) => MediaKind::Video(VideoCodec::H264),
                DecoderOptions::Aac(_) => MediaKind::Audio(AudioCodec::Aac),
            },
            present,
        };
        (chunk, sample_duration)
    }
}
