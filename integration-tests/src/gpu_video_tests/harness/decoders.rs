use crossbeam_channel::Receiver;
use ffmpeg_next::{
    codec::{Context as FfmpegContext, Id},
    frame,
    media::Type,
};
use gpu_video::{
    BytesDecoderH264, EncodedInputChunk, OutputFrame, RawFrameData, VideoDecoderError, VideoDevice,
    WgpuTexturesDecoderH264,
    parameters::DecoderParameters,
    parser::h264::{AccessUnit, H264Parser},
};

use crate::{
    gpu_video_tests::{Nv12Frame, harness::decode_test_runner::Decoder, video_device},
    video_decoder::copy_plane_from_av,
};

pub(super) struct GvBytesDecoderH264 {
    decoder: BytesDecoderH264,
    frame_receiver: Receiver<Result<OutputFrame<RawFrameData>, VideoDecoderError>>,
}

impl GvBytesDecoderH264 {
    pub fn new(video_device: &VideoDevice, parameters: DecoderParameters) -> Self {
        let (frame_sender, frame_receiver) = crossbeam_channel::unbounded();
        let decoder = video_device
            .create_bytes_decoder_h264(parameters, move |result| frame_sender.send(result).unwrap())
            .unwrap();

        Self {
            decoder,
            frame_receiver,
        }
    }

    fn drain(&mut self) -> Vec<Nv12Frame> {
        self.frame_receiver
            .try_iter()
            .map(|result| {
                let frame = result.unwrap();
                Nv12Frame {
                    width: frame.data.width as usize,
                    height: frame.data.height as usize,
                    data: frame.data.frame,
                }
            })
            .collect()
    }
}

impl Decoder for GvBytesDecoderH264 {
    fn decoder_name(&self) -> &'static str {
        "BytesDecoderH264"
    }

    fn decode_bytes(&mut self, data: &[u8]) -> Vec<Nv12Frame> {
        self.decoder
            .decode(EncodedInputChunk { data, pts: None })
            .unwrap();
        self.drain()
    }

    fn flush_frames(&mut self) -> Vec<Nv12Frame> {
        self.decoder.flush().unwrap();
        self.drain()
    }
}

pub(super) struct GvWgpuTexturesDecoderH264 {
    decoder: WgpuTexturesDecoderH264,
    frame_receiver: Receiver<Result<OutputFrame<wgpu::Texture>, VideoDecoderError>>,
}

impl GvWgpuTexturesDecoderH264 {
    pub fn new(
        video_device: &VideoDevice,
        wgpu_queue: &wgpu::Queue,
        parameters: DecoderParameters,
    ) -> Self {
        let (frame_sender, frame_receiver) = crossbeam_channel::unbounded();
        let decoder = video_device
            .create_wgpu_textures_decoder_h264(wgpu_queue, parameters, move |result| {
                frame_sender.send(result).unwrap()
            })
            .unwrap();

        Self {
            decoder,
            frame_receiver,
        }
    }

    fn drain(&mut self) -> Vec<Nv12Frame> {
        let (device, queue) = video_device();

        self.frame_receiver
            .try_iter()
            .map(|result| download_nv12_texture(device, queue, result.unwrap().data))
            .collect()
    }
}

impl Decoder for GvWgpuTexturesDecoderH264 {
    fn decoder_name(&self) -> &'static str {
        "WgpuTexturesDecoderH264"
    }

    fn decode_bytes(&mut self, data: &[u8]) -> Vec<Nv12Frame> {
        self.decoder
            .decode(EncodedInputChunk { data, pts: None })
            .unwrap();
        self.drain()
    }

    fn flush_frames(&mut self) -> Vec<Nv12Frame> {
        self.decoder.flush().unwrap();
        self.drain()
    }
}

impl Decoder for FfmpegDecoderH264 {
    fn decoder_name(&self) -> &'static str {
        "FfmpegDecoderH264"
    }

    fn decode_bytes(&mut self, data: &[u8]) -> Vec<Nv12Frame> {
        let access_units = self.parser.parse(data, None).unwrap();

        let mut frames = Vec::new();
        self.send_access_units(access_units, &mut frames);
        frames
    }

    fn flush_frames(&mut self) -> Vec<Nv12Frame> {
        let access_units = self.parser.flush().unwrap();

        let mut frames = Vec::new();
        self.send_access_units(access_units, &mut frames);
        self.decoder.send_eof().unwrap();
        self.receive_ffmpeg_frames(&mut frames);
        frames
    }
}

pub(super) struct FfmpegDecoderH264 {
    parser: H264Parser,
    decoder: ffmpeg_next::decoder::Opened,
}

impl FfmpegDecoderH264 {
    pub fn new() -> Self {
        let mut parameters = ffmpeg_next::codec::Parameters::new();
        unsafe {
            let parameters = &mut *parameters.as_mut_ptr();
            parameters.codec_type = Type::Video.into();
            parameters.codec_id = Id::H264.into();
        };
        let decoder = FfmpegContext::from_parameters(parameters.clone())
            .unwrap()
            .decoder()
            .open_as(Into::<Id>::into(parameters.id()))
            .unwrap();

        Self {
            parser: H264Parser::default(),
            decoder,
        }
    }

    fn send_access_units(&mut self, access_units: Vec<AccessUnit>, frames: &mut Vec<Nv12Frame>) {
        for access_unit in access_units {
            let mut data = Vec::new();
            for nalu in access_unit.0.iter() {
                data.extend_from_slice(&nalu.raw_bytes);
            }

            let mut packet = ffmpeg_next::Packet::new(data.len());
            packet.data_mut().unwrap().copy_from_slice(&data);
            self.decoder.send_packet(&packet).unwrap();
            self.receive_ffmpeg_frames(frames);
        }
    }

    fn receive_ffmpeg_frames(&mut self, frames: &mut Vec<Nv12Frame>) {
        let mut decoded = frame::Video::empty();
        while self.decoder.receive_frame(&mut decoded).is_ok() {
            let width = decoded.width() as usize;
            let height = decoded.height() as usize;

            let y_plane = copy_plane_from_av(&decoded, 0);
            let u_plane = copy_plane_from_av(&decoded, 1);
            let v_plane = copy_plane_from_av(&decoded, 2);

            let mut data = Vec::with_capacity(width * height * 3 / 2);
            data.extend_from_slice(&y_plane);
            for (u, v) in u_plane.iter().zip(v_plane.iter()) {
                data.push(*u);
                data.push(*v);
            }

            frames.push(Nv12Frame {
                width,
                height,
                data,
            });
        }
    }
}

fn download_nv12_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: wgpu::Texture,
) -> Nv12Frame {
    let width = texture.width();
    let height = texture.height();

    let bytes_per_row = (width as u64).next_multiple_of(256);
    let y_plane_size = bytes_per_row * height as u64;
    let uv_plane_size = bytes_per_row * height as u64 / 2;

    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("nv12 download buffer"),
        size: y_plane_size + uv_plane_size,
        usage: wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            aspect: wgpu::TextureAspect::Plane0,
            origin: wgpu::Origin3d::ZERO,
            texture: &texture,
            mip_level: 0,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(bytes_per_row as u32),
                rows_per_image: None,
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            aspect: wgpu::TextureAspect::Plane1,
            origin: wgpu::Origin3d::ZERO,
            texture: &texture,
            mip_level: 0,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: y_plane_size,
                bytes_per_row: Some(bytes_per_row as u32),
                rows_per_image: None,
            },
        },
        wgpu::Extent3d {
            width: width / 2,
            height: height / 2,
            depth_or_array_layers: 1,
        },
    );

    queue.submit(Some(encoder.finish()));

    let y = read_buffer(
        device,
        queue,
        &buffer,
        0,
        y_plane_size,
        bytes_per_row,
        width,
    );
    let uv = read_buffer(
        device,
        queue,
        &buffer,
        y_plane_size,
        y_plane_size + uv_plane_size,
        bytes_per_row,
        width,
    );

    let mut data = y;
    data.extend(uv);
    Nv12Frame {
        width: width as usize,
        height: height as usize,
        data,
    }
}

fn read_buffer(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    buffer: &wgpu::Buffer,
    start: u64,
    end: u64,
    bytes_per_row: u64,
    row_width: u32,
) -> Vec<u8> {
    let (tx, rx) = std::sync::mpsc::channel();
    wgpu::util::DownloadBuffer::read_buffer(device, queue, &buffer.slice(start..end), move |buf| {
        let buf = buf.unwrap();
        let mut result = Vec::new();
        for chunk in buf.chunks(bytes_per_row as usize) {
            result.extend_from_slice(&chunk[..row_width as usize]);
        }
        tx.send(result).unwrap();
    });
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    rx.recv().unwrap()
}
