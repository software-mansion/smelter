use std::{
    fs::File,
    io::{Read, Seek},
    os::unix::fs::MetadataExt,
    path::Path,
    time::Duration,
};

use bytes::Bytes;
use mp4::Mp4Sample;
use tracing::warn;

use crate::{pipeline::decoder::h264_utils::H264AvcDecoderConfig, prelude::*};

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

    pub fn find_aac_track(self) -> Option<Track<Reader>> {
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

        Some(Track {
            sample_count: track.sample_count(),
            timescale: track.timescale(),
            track_id,
            duration: track.duration(),
            decoder_options: DecoderOptions::Aac(asc),
            reader: self.reader,
        })
    }

    pub fn find_h264_track(self) -> Option<Track<Reader>> {
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

        Some(Track {
            sample_count: track.sample_count(),
            timescale: track.timescale(),
            track_id,
            duration: track.duration(),
            decoder_options: DecoderOptions::H264(h264_config),
            reader: self.reader,
        })
    }
}

pub(crate) struct Track<Reader: Read + Seek + Send + 'static> {
    reader: mp4::Mp4Reader<Reader>,
    sample_count: u32,
    timescale: u32,
    track_id: u32,
    duration: Duration,
    decoder_options: DecoderOptions,
}

impl<Reader: Read + Seek + Send + 'static> Track<Reader> {
    pub(crate) fn chunks(&mut self) -> TrackChunks<'_, Reader> {
        TrackChunks {
            track: self,
            last_sample_index: 1,
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
}

pub(crate) struct TrackChunks<'a, Reader: Read + Seek + Send + 'static> {
    track: &'a mut Track<Reader>,
    last_sample_index: u32,
}

impl<Reader: Read + Seek + Send + 'static> Iterator for TrackChunks<'_, Reader> {
    type Item = (EncodedInputChunk, Duration);

    fn next(&mut self) -> Option<Self::Item> {
        while self.last_sample_index < self.track.sample_count {
            let sample = self
                .track
                .reader
                .read_sample(self.track.track_id, self.last_sample_index);
            self.last_sample_index += 1;
            match sample {
                Ok(Some(sample)) => return Some(self.sample_into_chunk(sample)),
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
    fn sample_into_chunk(&mut self, sample: Mp4Sample) -> (EncodedInputChunk, Duration) {
        let rendering_offset = sample.rendering_offset;
        let start_time = sample.start_time;
        let sample_duration =
            Duration::from_secs_f64(sample.duration as f64 / self.track.timescale as f64);

        let dts = Duration::from_secs_f64(start_time as f64 / self.track.timescale as f64);
        let pts = Duration::from_secs_f64(
            (start_time as f64 + rendering_offset as f64) / self.track.timescale as f64,
        );

        let chunk = EncodedInputChunk {
            data: sample.bytes,
            pts,
            dts: Some(dts),
            kind: match self.track.decoder_options {
                DecoderOptions::H264(_) => MediaKind::Video(VideoCodec::H264),
                DecoderOptions::Aac(_) => MediaKind::Audio(AudioCodec::Aac),
            },
        };
        (chunk, sample_duration)
    }
}
