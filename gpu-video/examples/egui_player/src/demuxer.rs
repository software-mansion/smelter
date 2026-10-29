use std::{fs::File, io::BufReader, path::Path, time::Duration};

use mp4::{MediaType, Mp4Reader, Mp4Sample, TrackType};

const START_CODE: [u8; 4] = [0, 0, 0, 1];

pub struct Chunk {
    pub data: Vec<u8>,
    pub pts: Duration,
}

pub struct Mp4Demuxer {
    mp4: Mp4Reader<BufReader<File>>,
    track_id: u32,
    timescale: u32,
    sample_count: u32,
    next_sample_id: u32,
    nalu_length_size: usize,
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

        let mut parameter_sets = Vec::new();
        for nalu in avcc
            .sequence_parameter_sets
            .iter()
            .chain(&avcc.picture_parameter_sets)
        {
            parameter_sets.extend_from_slice(&START_CODE);
            parameter_sets.extend_from_slice(&nalu.bytes);
        }

        Self {
            track_id,
            timescale: track.timescale(),
            sample_count: track.sample_count(),
            next_sample_id: 1,
            nalu_length_size: avcc.length_size_minus_one as usize + 1,
            parameter_sets,
            mp4,
        }
    }

    pub fn next_chunk(&mut self) -> Option<Chunk> {
        if self.next_sample_id > self.sample_count {
            return None;
        }

        let sample = self
            .mp4
            .read_sample(self.track_id, self.next_sample_id)
            .unwrap()
            .unwrap();
        self.next_sample_id += 1;

        let pts = (sample.start_time as i64 + sample.rendering_offset as i64) as f64;
        Some(Chunk {
            data: self.avcc_to_annexb(&sample),
            pts: Duration::from_secs_f64(pts / self.timescale as f64),
        })
    }

    fn avcc_to_annexb(&self, sample: &Mp4Sample) -> Vec<u8> {
        let mut data = Vec::new();
        if sample.is_sync {
            data.extend_from_slice(&self.parameter_sets);
        }

        let mut rest = &sample.bytes[..];
        while !rest.is_empty() {
            let (length, tail) = rest.split_at(self.nalu_length_size);
            let length = u32::from_be_bytes(length.try_into().unwrap()) as usize;
            data.extend_from_slice(&START_CODE);
            data.extend_from_slice(&tail[..length]);
            rest = &tail[length..];
        }

        data
    }
}
