use std::{
    fs::File,
    io::{Read, Seek},
    os::unix::fs::MetadataExt,
    path::Path,
    time::Duration,
};

use bytes::{Buf, Bytes, BytesMut};
use mp4::Mp4Sample;
use tracing::warn;

use crate::prelude::*;

pub(super) struct Mp4FileReader<Reader: Read + Seek + Send + 'static> {
    reader: mp4::Mp4Reader<Reader>,
}

#[derive(Debug, Clone)]
pub(super) enum DecoderOptions {
    H264,
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
            sample_unpacker: Box::new(|sample| sample.bytes),
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

        // sps and pps have to be extracted from the container, interleaved with [0, 0, 0, 1],
        // concatenated and prepended to the first frame.
        let sps = avc
            .avcc
            .sequence_parameter_sets
            .iter()
            .flat_map(|s| [0, 0, 0, 1].iter().chain(s.bytes.iter()));

        let pps = avc
            .avcc
            .picture_parameter_sets
            .iter()
            .flat_map(|s| [0, 0, 0, 1].iter().chain(s.bytes.iter()));

        let mut sps_and_pps_payload = Some(sps.chain(pps).copied().collect::<Bytes>());

        let length_size = avc.avcc.length_size_minus_one + 1;

        let sample_unpacker = move |sample: mp4::Mp4Sample| {
            let mut sample_data = sample.bytes.reader();
            let mut data: BytesMut = Default::default();

            if let Some(first_nal) = sps_and_pps_payload.take() {
                data.extend_from_slice(&first_nal);
            }

            // the mp4 sample contains one h264 access unit (possibly more than one NAL).
            // the NALs are stored as: <length_size bytes long big endian encoded length><the NAL>.
            // we need to convert this into Annex B, in which NALs are separated by
            // [0, 0, 0, 1]. `length_size` is at most 4 bytes long.
            loop {
                let mut len = [0u8; 4];

                if sample_data
                    .read_exact(&mut len[4 - length_size as usize..])
                    .is_err()
                {
                    break;
                }

                let len = u32::from_be_bytes(len);

                let mut nalu = bytes::BytesMut::zeroed(len as usize);
                sample_data.read_exact(&mut nalu).unwrap();

                data.extend_from_slice(&[0, 0, 0, 1]);
                data.extend_from_slice(&nalu);
            }

            data.freeze()
        };

        Some(Track {
            sample_unpacker: Box::new(sample_unpacker),
            sample_count: track.sample_count(),
            timescale: track.timescale(),
            track_id,
            duration: track.duration(),
            decoder_options: DecoderOptions::H264,
            reader: self.reader,
        })
    }
}

pub(crate) struct Track<Reader: Read + Seek + Send + 'static> {
    reader: mp4::Mp4Reader<Reader>,
    sample_unpacker: Box<dyn FnMut(mp4::Mp4Sample) -> Bytes + Send>,
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

        let data = (self.track.sample_unpacker)(sample);

        let chunk = EncodedInputChunk {
            data,
            pts,
            dts: Some(dts),
            kind: match self.track.decoder_options {
                DecoderOptions::H264 => MediaKind::Video(VideoCodec::H264),
                DecoderOptions::Aac(_) => MediaKind::Audio(AudioCodec::Aac),
            },
        };
        (chunk, sample_duration)
    }
}
