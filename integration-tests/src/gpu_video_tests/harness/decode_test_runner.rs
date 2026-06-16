use std::{collections::VecDeque, fs::File, io::Read};

use gpu_video::{VideoDeviceExt, parameters::DecoderParameters};

use crate::{
    gpu_video_tests::{Nv12Frame, TestCase, harness::decoders::FfmpegDecoderH264, video_device},
    paths::gpu_video_dumps_dir_path,
};

impl TestCase<DecoderOptions> {
    pub fn run(&self) {
        let (device, _) = video_device();
        let video_device = device.video().unwrap();

        let (reference_decoder, gv_decoders) = match self.options {
            DecoderOptions::H264(params) => (
                BufferedDecoder::from_decoder(FfmpegDecoderH264::new()),
                vec![
                    BufferedDecoder::from_decoder(
                        video_device.create_bytes_decoder_h264(params).unwrap(),
                    ),
                    BufferedDecoder::from_decoder(
                        video_device
                            .create_wgpu_textures_decoder_h264(params)
                            .unwrap(),
                    ),
                ],
            ),
        };

        self.verify_decoders(reference_decoder, gv_decoders);
    }

    fn verify_decoders(
        &self,
        mut reference_decoder: BufferedDecoder,
        mut gv_decoders: Vec<BufferedDecoder>,
    ) {
        let mut source = File::open(gpu_video_dumps_dir_path().join(&self.dump_file_path)).unwrap();

        let mut buffer = [0; 4096];
        while let Ok(n) = source.read(&mut buffer) {
            match n {
                0 => {
                    reference_decoder.flush_frames();
                    for decoder in gv_decoders.iter_mut() {
                        decoder.flush_frames();
                    }
                }
                _ => {
                    reference_decoder.decode_bytes(&buffer[..n]);
                    for decoder in gv_decoders.iter_mut() {
                        decoder.decode_bytes(&buffer[..n]);
                    }
                }
            }

            while reference_decoder.has_next_frame()
                && gv_decoders.iter().all(BufferedDecoder::has_next_frame)
            {
                let reference_frame = reference_decoder.next_frame().unwrap();
                for decoder in gv_decoders.iter_mut() {
                    let actual_frame = decoder.next_frame().unwrap();
                    self.compare(decoder.decoder_name(), &reference_frame, &actual_frame);
                }
            }

            if n == 0 {
                break;
            }
        }

        for decoder in gv_decoders {
            assert_eq!(
                reference_decoder.frame_count,
                decoder.frame_count,
                "Test {:?} ({}): Wrong number of decoded frames, (actual) {} != (expected) {}",
                self.dump_file_path,
                decoder.decoder_name(),
                decoder.frame_count,
                reference_decoder.frame_count,
            );
        }
    }

    fn compare(
        &self,
        decoder_name: &str,
        (reference_frame, reference_frame_idx): &(Nv12Frame, usize),
        (actual_frame, actual_frame_idx): &(Nv12Frame, usize),
    ) {
        assert_eq!(actual_frame_idx, reference_frame_idx);

        assert_eq!(
            (actual_frame.width, actual_frame.height),
            (reference_frame.width, reference_frame.height),
            "Test {:?} ({decoder_name}, frame {actual_frame_idx}): actual resolution {}x{} != expected resolution {}x{}",
            self.dump_file_path,
            actual_frame.width,
            actual_frame.height,
            reference_frame.width,
            reference_frame.height,
        );

        let diff = frame_data_diff(&actual_frame.data, &reference_frame.data);
        assert!(
            diff <= self.allowed_error,
            "Test {:?} ({decoder_name}, frame {actual_frame_idx}): diff {diff} exceeds allowed error {}",
            self.dump_file_path,
            self.allowed_error
        );
    }
}

fn frame_data_diff(actual: &[u8], expected: &[u8]) -> f32 {
    if actual.len() != expected.len() {
        return f32::MAX;
    }

    let diff = actual.iter().zip(expected).fold(0.0, |total_err, (a, b)| {
        total_err + (*a as f32 - *b as f32).powf(2.0)
    });

    diff / actual.len() as f32
}

pub enum DecoderOptions {
    H264(DecoderParameters),
}

struct BufferedDecoder {
    frames: VecDeque<(Nv12Frame, usize)>,
    frame_count: usize,
    decoder: Box<dyn Decoder>,
}

impl BufferedDecoder {
    fn from_decoder<D: Decoder + 'static>(decoder: D) -> Self {
        Self {
            frames: VecDeque::new(),
            frame_count: 0,
            decoder: Box::new(decoder) as Box<dyn Decoder>,
        }
    }

    fn decoder_name(&self) -> &'static str {
        self.decoder.decoder_name()
    }

    fn decode_bytes(&mut self, data: &[u8]) {
        let frames = self.decoder.decode_bytes(data);
        self.push_frames(frames);
    }

    fn flush_frames(&mut self) {
        let frames = self.decoder.flush_frames();
        self.push_frames(frames);
    }

    fn push_frames(&mut self, frames: Vec<Nv12Frame>) {
        let frames = frames.into_iter().map(|frame| {
            self.frame_count += 1;
            (frame, self.frame_count - 1)
        });
        self.frames.extend(frames);
    }

    fn next_frame(&mut self) -> Option<(Nv12Frame, usize)> {
        self.frames.pop_front()
    }

    fn has_next_frame(&self) -> bool {
        !self.frames.is_empty()
    }
}

pub(super) trait Decoder {
    fn decoder_name(&self) -> &'static str;

    fn decode_bytes(&mut self, data: &[u8]) -> Vec<Nv12Frame>;
    fn flush_frames(&mut self) -> Vec<Nv12Frame>;
}
