// TODO: reimport from egui
//
//
//
use std::{fs::File, io::BufReader, path::Path, time::Duration};

use gpu_video::parameters::AvccPrefixLength;
use mp4::{MediaType, Mp4Reader, TrackType};

pub struct Chunk {
    pub data: Vec<u8>,
    pub pts: Duration,
}

pub struct Mp4Demuxer {
    mp4: Mp4Reader<BufReader<File>>,
    track_id: u32,
    timescale: u32,
    next_sample_id: u32,
    pub prefix_length: AvccPrefixLength,
    parameter_sets: Vec<u8>,
}

impl Mp4Demuxer {
    pub fn new(path: &Path) -> Self {
        let file = File::open(path).unwrap();
        let size = file.metadata().unwrap().len();
        let mp4 = Mp4Reader::read_header(BufReader::new(file), size).unwrap();

        let (&track_id, track) = mp4
            .tracks()
            .iter()
            .find(|(_, track)| track.track_type().is_ok_and(|t| t == TrackType::Video))
            .expect("no video track");

        let media_type = track.media_type().expect("unsupported codec");
        assert_eq!(
            media_type,
            MediaType::H264,
            "unsupported codec: {media_type}"
        );

        let avcc = &track.trak.mdia.minf.stbl.stsd.avc1.as_ref().unwrap().avcc;
        let prefix_length = match avcc.length_size_minus_one {
            0 => AvccPrefixLength::Bytes1,
            1 => AvccPrefixLength::Bytes2,
            3 => AvccPrefixLength::Bytes4,
            n => panic!("invalid NALU length size: {}", n + 1),
        };

        let mut parameter_sets = Vec::new();
        for nalu in avcc
            .sequence_parameter_sets
            .iter()
            .chain(&avcc.picture_parameter_sets)
        {
            let len = (nalu.bytes.len() as u32).to_be_bytes();
            parameter_sets.extend_from_slice(&len[4 - prefix_length.prefix_byte_length()..]);
            parameter_sets.extend_from_slice(&nalu.bytes);
        }

        Self {
            track_id,
            timescale: track.timescale(),
            next_sample_id: 1,
            prefix_length,
            parameter_sets,
            mp4,
        }
    }

    pub fn next_chunk(&mut self) -> Option<Chunk> {
        let sample = self
            .mp4
            .read_sample(self.track_id, self.next_sample_id)
            .unwrap()?;
        self.next_sample_id += 1;

        let mut data = std::mem::take(&mut self.parameter_sets);
        data.extend_from_slice(&sample.bytes);

        let pts =
            (sample.start_time as f64 + sample.rendering_offset as f64) / self.timescale as f64;
        Some(Chunk {
            data,
            pts: Duration::from_secs_f64(pts),
        })
    }
}
