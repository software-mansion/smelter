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

        let (track_start_offset, presentation_delay) =
            Self::calculate_elst_edits(track, self.reader.timescale());

        Some(Track {
            sample_count: track.sample_count(),
            timescale: track.timescale(),
            track_id,
            duration: track.duration(),
            decoder_options: DecoderOptions::Aac(asc),
            track_start_offset,
            presentation_delay,
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
            track_start_offset: offset,
            presentation_delay: delay,
            reader: self.reader,
        })
    }

    /// Calculates the media-time offset and presentation delay from the edit list.
    ///
    /// Returns `(offset, delay)` where:
    /// - `track_start_offset` - derived from `media_time` of the first non-empty edit. Contains information
    ///   how much time should be cut from the beginning of the track.
    /// - `presentation_delay` - the sum of `duration` of all leading empty edits. Contains information on how
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
        let mut track_start_offset = Duration::ZERO;

        for entry in entries {
            // u32::MAX value is the result of overflowing -1
            if entry.media_time == u32::MAX as u64 || entry.media_time == u64::MAX {
                delay_ticks += entry.segment_duration;
            } else {
                track_start_offset =
                    Duration::from_secs_f64(entry.media_time as f64 / track.timescale() as f64);
                break;
            }
        }

        let presentation_delay =
            Duration::from_secs_f64(delay_ticks as f64 / movie_timescale as f64);
        (track_start_offset, presentation_delay)
    }
}

pub(crate) struct Track<Reader: Read + Seek + Send + 'static> {
    reader: mp4::Mp4Reader<Reader>,
    sample_count: u32,
    timescale: u32,
    track_id: u32,
    duration: Duration,
    decoder_options: DecoderOptions,

    /// How much time should be cut from the beginning of the track, derived from the `media_time`
    /// field of the first non-empty edit in the `elst` box.
    track_start_offset: Duration,

    /// How much the track presentation should be delayed, derived from the summed `segment_duration`
    /// of all leading empty edits (`media_time == -1`) in the `elst` box.
    presentation_delay: Duration,
}

impl<Reader: Read + Seek + Send + 'static> Track<Reader> {
    pub(crate) fn chunks(&mut self, seek: Option<Duration>) -> TrackChunks<'_, Reader> {
        let user_seek = seek.unwrap_or(Duration::ZERO);

        // Position on the raw media-sample timeline (the STTS-described timeline where
        // each stored sample has a fixed time) from which sample reading should start.
        // Computed so that any portion of `user_seek` that falls inside the leading
        // `presentation_delay` (black screen) is absorbed by that black screen instead
        // of being skipped over real samples. When `user_seek <= presentation_delay`
        // this collapses to `track_start_offset`, meaning "start at the very first
        // sample the `elst` box tells us to actually play"; excess seek beyond the
        // black screen advances further into media.
        let delayed_user_seek = user_seek.saturating_sub(self.presentation_delay);
        let media_seek = self.track_start_offset + delayed_user_seek;

        // Used in `sample_into_chunk` to shift each sample's pts so that the user's
        // seek point becomes pts 0. If the user seeks into the leading black screen
        // (user_seek < presentation_delay), the first real sample gets a positive pts
        // equal to the remaining black screen, which the pipeline fills with black.
        let track_seek = self.track_start_offset + user_seek;

        match self.find_seek_start_sample(media_seek) {
            Ok((start_index, present_index)) => TrackChunks {
                track: self,
                track_seek,
                next_sample_index: start_index,
                present_from_index: present_index,
            },
            Err(unpresentable) => TrackChunks {
                track: self,
                track_seek,
                next_sample_index: unpresentable,
                present_from_index: unpresentable,
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
    /// If seek is past the end, returns `Err` with index one past the last sample
    fn find_seek_start_sample(&self, seek: Duration) -> Result<(u32, u32), u32> {
        let seek_timestamp = (seek.as_secs_f64() * self.timescale as f64) as u64;
        let track = &self.reader.tracks()[&self.track_id];

        // The STTS box maps samples to batches of samples with the same length
        let stts = &track.trak.mdia.minf.stbl.stts;

        // The STSS box contains indices of sync samples (e.g. key frames).
        // `None` means all samples are sync samples.
        let stss = &track.trak.mdia.minf.stbl.stss;

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

        let present_from_index = match present_from_index {
            Some(pfi) => u32::max(pfi, 1),
            None => return Err(samples_skipped + 1),
        };

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

        Ok((sync_index, present_from_index))
    }
}

pub(crate) struct TrackChunks<'a, Reader: Read + Seek + Send + 'static> {
    track: &'a mut Track<Reader>,

    /// Value subtracted from each sample's pts to align presentation with the user's seek.
    /// Equals the user-provided seek time plus `track_start_offset` (the leading media trimmed
    /// by the `elst` box).
    track_seek: Duration,
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
        let presentation_delay = self.track.presentation_delay;

        let dts = Duration::from_secs_f64(start_time as f64 / self.track.timescale as f64);
        let mut pts = Duration::from_secs_f64(
            (start_time as f64 + rendering_offset as f64) / self.track.timescale as f64,
        );
        pts += presentation_delay;
        pts = pts.saturating_sub(self.track_seek);

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
