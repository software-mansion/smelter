use std::{collections::VecDeque, fs::File, io::Read, num::NonZeroU32, time::Duration};

use ffmpeg_next::{codec::Id, color};
use gpu_video::{
    EncodedOutputChunk, VideoAdapterExt, VideoDevice, VideoDeviceExt,
    parameters::{
        ColorRange, ColorSpace, EncoderOutputParameters, EncoderParametersH264,
        EncoderParametersH265, EncoderPreset, RateControl, VideoParameters,
    },
};

use crate::{
    gpu_video_tests::{
        Nv12Frame, TestCase, create_video_device, frame_data_diff,
        harness::{
            decoders::{Decoder, FfmpegDecoder, GvBytesDecoderH264, GvWgpuTexturesDecoderH264},
            encoders::{Encoder, EncoderParameters, GvBytesEncoder, GvWgpuTexturesEncoder},
        },
    },
    paths::gpu_video_dumps_dir_path,
};

const FRAMERATE: u32 = 25;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Codec {
    H264,
    H265,
}

pub struct EncoderOptions {
    pub codec: Codec,
    pub preset: EncoderPreset,
    pub rate_control: RateControl,
    pub idr_period: Option<NonZeroU32>,
    pub inline_stream_params: bool,
    pub color: Option<(ColorSpace, ColorRange)>,
    pub max_in_flight_submissions: Option<u32>,
    pub forced_keyframes: Vec<usize>,
    pub flush_at: Vec<usize>,
    pub concurrent: usize,
    pub max_frames: Option<usize>,
    pub bitrate_tolerance: Option<f64>,
}

impl EncoderOptions {
    pub fn new(codec: Codec) -> Self {
        Self {
            codec,
            preset: EncoderPreset::HighQuality,
            rate_control: RateControl::VariableBitrate {
                average_bitrate: 4_000_000,
                max_bitrate: 8_000_000,
                virtual_buffer_size: Duration::from_secs(2),
            },
            idr_period: None,
            inline_stream_params: true,
            color: None,
            max_in_flight_submissions: None,
            forced_keyframes: Vec::new(),
            flush_at: Vec::new(),
            concurrent: 0,
            max_frames: None,
            bitrate_tolerance: None,
        }
    }

    pub fn encoder_parameters(
        &self,
        video_device: &VideoDevice,
        width: u32,
        height: u32,
    ) -> EncoderParameters {
        let input_parameters = VideoParameters {
            width: NonZeroU32::new(width).unwrap(),
            height: NonZeroU32::new(height).unwrap(),
            target_framerate: FRAMERATE.into(),
        };
        match self.codec {
            Codec::H264 => EncoderParameters::H264(EncoderParametersH264 {
                input_parameters,
                output_parameters: self.output_parameters(
                    video_device
                        .encoder_output_parameters_h264_high_quality(self.rate_control)
                        .unwrap(),
                ),
                max_in_flight_submissions: self.max_in_flight_submissions,
            }),
            Codec::H265 => EncoderParameters::H265(EncoderParametersH265 {
                input_parameters,
                output_parameters: self.output_parameters(
                    video_device
                        .encoder_output_parameters_h265_high_quality(self.rate_control)
                        .unwrap(),
                ),
                max_in_flight_submissions: self.max_in_flight_submissions,
            }),
        }
    }

    fn output_parameters<P>(
        &self,
        parameters: EncoderOutputParameters<P>,
    ) -> EncoderOutputParameters<P> {
        EncoderOutputParameters {
            preset: self.preset,
            idr_period: self.idr_period,
            inline_stream_params: Some(self.inline_stream_params),
            color_space: self.color.map(|(space, _)| space),
            color_range: self.color.map(|(_, range)| range),
            ..parameters
        }
    }

    fn is_expected_keyframe(&self, frame_idx: usize) -> bool {
        frame_idx == 0
            || self.forced_keyframes.contains(&frame_idx)
            || self
                .idr_period
                .is_some_and(|period| frame_idx.is_multiple_of(period.get() as usize))
    }

    fn target_bitrate(&self) -> u64 {
        match self.rate_control {
            RateControl::VariableBitrate {
                average_bitrate, ..
            } => average_bitrate,
            RateControl::ConstantBitrate { bitrate, .. } => bitrate,
            RateControl::EncoderDefault | RateControl::Disabled => {
                panic!("Bitrate can only be verified with VBR or CBR rate control")
            }
        }
    }
}

pub fn create_encoding_device(codec: Codec) -> (wgpu::Device, wgpu::Queue) {
    create_video_device(|adapter| {
        adapter
            .video_adapter_info()
            .is_some_and(|info| match codec {
                Codec::H264 => info.encode_capabilities.h264.is_some(),
                Codec::H265 => info.encode_capabilities.h265.is_some(),
            })
    })
}

impl TestCase<EncoderOptions> {
    pub fn run(&self) {
        let (device, queue) = create_encoding_device(self.options.codec);

        let mut source_decoder = FfmpegDecoder::new(Id::H264);
        let mut concurrent_decoders: Vec<Box<dyn Decoder>> = match self.options.concurrent {
            0 => Vec::new(),
            _ => vec![
                Box::new(GvBytesDecoderH264::new(&device, Default::default())),
                Box::new(GvWgpuTexturesDecoderH264::new(
                    &device,
                    &queue,
                    Default::default(),
                )),
            ],
        };
        let mut encoders: Vec<VerifiedEncoder> = Vec::new();
        let mut source_frames = VecDeque::new();
        let mut source_frame_count = 0;

        let mut dump = File::open(gpu_video_dumps_dir_path().join(&self.dump_file_path)).unwrap();
        let mut buffer = [0; 4096];
        loop {
            let n = dump.read(&mut buffer).unwrap();
            let frames = match n {
                0 => source_decoder.flush_frames(),
                _ => source_decoder.decode_bytes(&buffer[..n]),
            };
            for decoder in concurrent_decoders.iter_mut() {
                match n {
                    0 => decoder.flush_frames(),
                    _ => decoder.decode_bytes(&buffer[..n]),
                };
            }

            for frame in frames {
                if self
                    .options
                    .max_frames
                    .is_some_and(|max| source_frame_count >= max)
                {
                    break;
                }
                if encoders.is_empty() {
                    encoders = self.create_encoders(&device, &queue, frame.width, frame.height);
                }

                let frame_idx = source_frame_count;
                let force_keyframe = self.options.forced_keyframes.contains(&frame_idx);
                for encoder in encoders.iter_mut() {
                    assert_eq!(
                        (frame.width, frame.height),
                        encoder.resolution,
                        "Test {:?}: Dumps with changing resolution are not supported by encoder tests",
                        self.dump_file_path,
                    );
                    let chunks = encoder
                        .encoder
                        .encode(&frame, frame_idx as u64, force_keyframe);
                    self.process_chunks(encoder, chunks);
                    if self.options.flush_at.contains(&frame_idx) {
                        let chunks = encoder.encoder.flush();
                        self.process_chunks(encoder, chunks);
                    }
                }

                source_frames.push_back((frame, frame_idx));
                source_frame_count += 1;
                self.compare_ready_frames(&mut source_frames, &mut encoders);
            }

            if n == 0 {
                break;
            }
        }

        for encoder in encoders.iter_mut() {
            let chunks = encoder.encoder.flush();
            self.process_chunks(encoder, chunks);
            let frames = encoder.reference_decoder.flush();
            encoder.decoded_frames.extend(frames);
        }
        self.compare_ready_frames(&mut source_frames, &mut encoders);

        for encoder in encoders {
            self.verify_summary(&encoder, source_frame_count);
        }
    }

    fn create_encoders(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        width: usize,
        height: usize,
    ) -> Vec<VerifiedEncoder> {
        let parameters =
            self.options
                .encoder_parameters(&device.video().unwrap(), width as u32, height as u32);
        let codec_id = match self.options.codec {
            Codec::H264 => Id::H264,
            Codec::H265 => Id::HEVC,
        };

        (0..=self.options.concurrent)
            .flat_map(|_| {
                [
                    Box::new(GvBytesEncoder::new(device, parameters)) as Box<dyn Encoder>,
                    Box::new(GvWgpuTexturesEncoder::new(device, queue, parameters)),
                ]
            })
            .map(|encoder| VerifiedEncoder {
                encoder,
                resolution: (width, height),
                reference_decoder: FfmpegDecoder::new(codec_id),
                decoded_frames: VecDeque::new(),
                chunk_count: 0,
                total_bytes: 0,
            })
            .collect()
    }

    fn process_chunks(
        &self,
        encoder: &mut VerifiedEncoder,
        chunks: Vec<EncodedOutputChunk<Vec<u8>>>,
    ) {
        let name = encoder.encoder.encoder_name();
        for chunk in chunks {
            let chunk_idx = encoder.chunk_count;
            encoder.chunk_count += 1;
            encoder.total_bytes += chunk.data.len();

            assert_eq!(
                chunk.pts,
                Some(chunk_idx as u64),
                "Test {:?} ({name}, chunk {chunk_idx}): wrong pts",
                self.dump_file_path,
            );

            let nal_types = nal_unit_types(self.options.codec, &chunk.data);
            assert_eq!(
                chunk.is_keyframe,
                nal_types
                    .iter()
                    .any(|t| is_keyframe_nal(self.options.codec, *t)),
                "Test {:?} ({name}, chunk {chunk_idx}): is_keyframe is {}, but NAL unit types are {nal_types:?}",
                self.dump_file_path,
                chunk.is_keyframe,
            );
            if self.options.is_expected_keyframe(chunk_idx) {
                assert!(
                    chunk.is_keyframe,
                    "Test {:?} ({name}, chunk {chunk_idx}): expected a keyframe",
                    self.dump_file_path,
                );
            }
            if self.options.idr_period.is_some() && self.options.forced_keyframes.is_empty() {
                assert_eq!(
                    chunk.is_keyframe,
                    self.options.is_expected_keyframe(chunk_idx),
                    "Test {:?} ({name}, chunk {chunk_idx}): keyframe doesn't match idr_period",
                    self.dump_file_path,
                );
            }

            let has_sps = nal_types.contains(&sps_nal_type(self.options.codec));
            if chunk.is_keyframe {
                assert_eq!(
                    has_sps,
                    self.options.inline_stream_params,
                    "Test {:?} ({name}, chunk {chunk_idx}): inline_stream_params is {}, but the keyframe {} SPS",
                    self.dump_file_path,
                    self.options.inline_stream_params,
                    if has_sps {
                        "contains"
                    } else {
                        "doesn't contain"
                    },
                );
            }

            let frames = match (chunk_idx, self.options.inline_stream_params) {
                (0, false) => encoder
                    .reference_decoder
                    .decode(&[encoder.encoder.stream_params(), chunk.data].concat()),
                _ => encoder.reference_decoder.decode(&chunk.data),
            };
            encoder.decoded_frames.extend(frames);
        }
    }

    fn compare_ready_frames(
        &self,
        source_frames: &mut VecDeque<(Nv12Frame, usize)>,
        encoders: &mut [VerifiedEncoder],
    ) {
        while !source_frames.is_empty() && encoders.iter().all(|e| !e.decoded_frames.is_empty()) {
            let (source_frame, frame_idx) = source_frames.pop_front().unwrap();
            for encoder in encoders.iter_mut() {
                let decoded = encoder.decoded_frames.pop_front().unwrap();
                self.compare(
                    encoder.encoder.encoder_name(),
                    frame_idx,
                    &source_frame,
                    decoded,
                );
            }
        }
    }

    fn compare(
        &self,
        encoder_name: &str,
        frame_idx: usize,
        source_frame: &Nv12Frame,
        (decoded_frame, color_space, color_range): (Nv12Frame, color::Space, color::Range),
    ) {
        assert_eq!(
            (decoded_frame.width, decoded_frame.height),
            (source_frame.width, source_frame.height),
            "Test {:?} ({encoder_name}, frame {frame_idx}): wrong resolution of the encoded frame",
            self.dump_file_path,
        );

        let diff = frame_data_diff(&decoded_frame.data, &source_frame.data);
        assert!(
            diff <= self.allowed_error,
            "Test {:?} ({encoder_name}, frame {frame_idx}): diff {diff} exceeds allowed error {}",
            self.dump_file_path,
            self.allowed_error
        );

        if let Some((expected_space, expected_range)) = self.options.color {
            let expected_space = match expected_space {
                ColorSpace::Unspecified => color::Space::Unspecified,
                ColorSpace::BT709 => color::Space::BT709,
                ColorSpace::BT601Ntsc => color::Space::SMPTE170M,
                ColorSpace::BT601Pal => color::Space::BT470BG,
            };
            let expected_range = match expected_range {
                ColorRange::Full => color::Range::JPEG,
                ColorRange::Limited => color::Range::MPEG,
            };
            assert_eq!(
                (color_space, color_range),
                (expected_space, expected_range),
                "Test {:?} ({encoder_name}, frame {frame_idx}): wrong color metadata",
                self.dump_file_path,
            );
        }
    }

    fn verify_summary(&self, encoder: &VerifiedEncoder, source_frame_count: usize) {
        let name = encoder.encoder.encoder_name();
        assert_eq!(
            encoder.chunk_count, source_frame_count,
            "Test {:?} ({name}): wrong number of encoded chunks",
            self.dump_file_path,
        );
        assert!(
            encoder.decoded_frames.is_empty(),
            "Test {:?} ({name}): more decoded frames than source frames",
            self.dump_file_path,
        );

        if let Some(tolerance) = self.options.bitrate_tolerance {
            let duration = source_frame_count as f64 / FRAMERATE as f64;
            let bitrate = encoder.total_bytes as f64 * 8.0 / duration;
            let target = self.options.target_bitrate() as f64;
            assert!(
                (bitrate - target).abs() / target <= tolerance,
                "Test {:?} ({name}): bitrate {bitrate:.0} is not within {tolerance} of the target {target}",
                self.dump_file_path,
            );
        }
    }

    pub fn run_drop_without_flush(&self, frame_count: usize) {
        let (device, queue) = create_encoding_device(self.options.codec);

        let mut source_decoder = FfmpegDecoder::new(Id::H264);
        let mut frames = Vec::new();
        let mut dump = File::open(gpu_video_dumps_dir_path().join(&self.dump_file_path)).unwrap();
        let mut buffer = [0; 4096];
        while frames.len() < frame_count.max(1) {
            let n = dump.read(&mut buffer).unwrap();
            match n {
                0 => panic!("Dump {:?} is too short", self.dump_file_path),
                _ => frames.extend(source_decoder.decode_bytes(&buffer[..n])),
            }
        }
        let mut encoders = self.create_encoders(&device, &queue, frames[0].width, frames[0].height);
        frames.truncate(frame_count);
        for (frame_idx, frame) in frames.iter().enumerate() {
            for encoder in encoders.iter_mut() {
                encoder.encoder.encode(frame, frame_idx as u64, false);
            }
        }
        drop(encoders);
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    }
}

struct VerifiedEncoder {
    encoder: Box<dyn Encoder>,
    resolution: (usize, usize),
    reference_decoder: FfmpegDecoder,
    decoded_frames: VecDeque<(Nv12Frame, color::Space, color::Range)>,
    chunk_count: usize,
    total_bytes: usize,
}

fn nal_unit_types(codec: Codec, data: &[u8]) -> Vec<u8> {
    data.windows(4)
        .filter(|w| w[..3] == [0, 0, 1])
        .map(|w| match codec {
            Codec::H264 => w[3] & 0x1f,
            Codec::H265 => (w[3] >> 1) & 0x3f,
        })
        .collect()
}

fn is_keyframe_nal(codec: Codec, nal_type: u8) -> bool {
    match codec {
        Codec::H264 => nal_type == 5,
        Codec::H265 => (16..=21).contains(&nal_type),
    }
}

fn sps_nal_type(codec: Codec) -> u8 {
    match codec {
        Codec::H264 => 7,
        Codec::H265 => 33,
    }
}
