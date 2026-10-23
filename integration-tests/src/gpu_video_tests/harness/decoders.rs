use crossbeam_channel::Receiver;
use ffmpeg_next::{
    codec::{Context as FfmpegContext, Id},
    color,
    ffi::{
        AV_NOPTS_VALUE, AVCodecParserContext, av_parser_close, av_parser_init, av_parser_parse2,
    },
    frame,
    media::Type,
};
use gpu_video::{
    BytesDecoderH264, EncodedInputChunk, OutputFrame, RawFrameData, VideoDeviceExt, VideoTexture,
    WgpuTexturesDecoderH264,
    parameters::{ColorRange, ColorSpace, DecoderParameters},
};

use crate::{gpu_video_tests::Nv12Frame, video_decoder::copy_plane_from_av};

pub(super) trait Decoder {
    fn decoder_name(&self) -> &'static str;

    fn decode_bytes(&mut self, data: &[u8]) -> Vec<Nv12Frame>;
    fn flush_frames(&mut self) -> Vec<Nv12Frame>;
}

pub(super) struct GvBytesDecoder {
    decoder: BytesDecoderH264,
    frame_receiver: Receiver<OutputFrame<RawFrameData>>,
}

impl GvBytesDecoder {
    pub fn new(wgpu_device: &wgpu::Device, parameters: DecoderParameters) -> Self {
        let (frame_sender, frame_receiver) = crossbeam_channel::unbounded();
        let decoder = wgpu_device
            .video()
            .unwrap()
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
            .map(|frame| Nv12Frame {
                width: frame.data.width as usize,
                height: frame.data.height as usize,
                data: frame.data.frame,
                color_space: frame.metadata.color_space,
                color_range: frame.metadata.color_range,
            })
            .collect()
    }
}

impl Decoder for GvBytesDecoder {
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

pub(super) struct GvWgpuTexturesDecoder {
    decoder: WgpuTexturesDecoderH264,
    frame_receiver: Receiver<OutputFrame<VideoTexture>>,
    wgpu_device: wgpu::Device,
    wgpu_queue: wgpu::Queue,
}

impl GvWgpuTexturesDecoder {
    pub fn new(
        wgpu_device: &wgpu::Device,
        wgpu_queue: &wgpu::Queue,
        parameters: DecoderParameters,
    ) -> Self {
        let (frame_sender, frame_receiver) = crossbeam_channel::unbounded();
        let decoder = wgpu_device
            .video()
            .unwrap()
            .create_wgpu_textures_decoder_h264(wgpu_queue, parameters, move |frame| {
                frame_sender.send(frame).unwrap()
            })
            .unwrap();

        Self {
            decoder,
            frame_receiver,
            wgpu_device: wgpu_device.clone(),
            wgpu_queue: wgpu_queue.clone(),
        }
    }

    fn drain(&mut self) -> Vec<Nv12Frame> {
        self.frame_receiver
            .try_iter()
            .map(|frame| download_nv12_texture(&self.wgpu_device, &self.wgpu_queue, frame))
            .collect()
    }
}

impl Decoder for GvWgpuTexturesDecoder {
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

impl Decoder for FfmpegDecoder {
    fn decoder_name(&self) -> &'static str {
        "FfmpegDecoder"
    }

    fn decode_bytes(&mut self, data: &[u8]) -> Vec<Nv12Frame> {
        let access_units = self.parser.parse(&mut self.decoder, data);
        access_units
            .iter()
            .flat_map(|access_unit| self.decode_access_unit(access_unit))
            .collect()
    }

    fn flush_frames(&mut self) -> Vec<Nv12Frame> {
        let mut frames = Vec::new();
        if let Some(access_unit) = self.parser.flush(&mut self.decoder) {
            frames.extend(self.decode_access_unit(&access_unit));
        }
        self.decoder.send_eof().unwrap();
        frames.extend(self.receive_frames());
        frames
    }
}

pub(super) struct FfmpegDecoder {
    parser: FfmpegParser,
    decoder: ffmpeg_next::decoder::Opened,
}

impl FfmpegDecoder {
    pub fn new(codec_id: Id) -> Self {
        let mut parameters = ffmpeg_next::codec::Parameters::new();
        unsafe {
            let parameters = &mut *parameters.as_mut_ptr();
            parameters.codec_type = Type::Video.into();
            parameters.codec_id = codec_id.into();
        };
        let decoder = FfmpegContext::from_parameters(parameters)
            .unwrap()
            .decoder()
            .open_as(codec_id)
            .unwrap();

        Self {
            parser: FfmpegParser::new(codec_id),
            decoder,
        }
    }

    fn decode_access_unit(&mut self, access_unit: &ffmpeg_next::Packet) -> Vec<Nv12Frame> {
        self.decoder.send_packet(access_unit).unwrap();
        self.receive_frames()
    }

    fn receive_frames(&mut self) -> Vec<Nv12Frame> {
        let mut frames = Vec::new();
        let mut decoded = frame::Video::empty();
        while self.decoder.receive_frame(&mut decoded).is_ok() {
            frames.push(nv12_frame_from_av(&decoded));
        }
        frames
    }
}

struct FfmpegParser(*mut AVCodecParserContext);

impl FfmpegParser {
    fn new(codec_id: Id) -> Self {
        let parser = unsafe { av_parser_init(codec_id.into()) };
        assert!(!parser.is_null());
        Self(parser)
    }

    fn parse(
        &mut self,
        decoder: &mut ffmpeg_next::decoder::Opened,
        mut data: &[u8],
    ) -> Vec<ffmpeg_next::Packet> {
        let mut access_units = Vec::new();
        while !data.is_empty() {
            let (consumed, access_unit) = self.parse_next(decoder, data);
            data = &data[consumed..];
            access_units.extend(access_unit);
        }
        access_units
    }

    fn flush(&mut self, decoder: &mut ffmpeg_next::decoder::Opened) -> Option<ffmpeg_next::Packet> {
        let (_, access_unit) = self.parse_next(decoder, &[]);
        access_unit
    }

    fn parse_next(
        &mut self,
        decoder: &mut ffmpeg_next::decoder::Opened,
        data: &[u8],
    ) -> (usize, Option<ffmpeg_next::Packet>) {
        let mut output = std::ptr::null_mut();
        let mut output_size = 0;
        let consumed = unsafe {
            av_parser_parse2(
                self.0,
                decoder.as_mut_ptr(),
                &mut output,
                &mut output_size,
                data.as_ptr(),
                data.len() as i32,
                AV_NOPTS_VALUE,
                AV_NOPTS_VALUE,
                0,
            )
        };
        let access_unit = (output_size > 0).then(|| {
            let data = unsafe { std::slice::from_raw_parts(output, output_size as usize) };
            ffmpeg_next::Packet::copy(data)
        });
        (consumed as usize, access_unit)
    }
}

impl Drop for FfmpegParser {
    fn drop(&mut self) {
        unsafe { av_parser_close(self.0) };
    }
}

fn nv12_frame_from_av(decoded: &frame::Video) -> Nv12Frame {
    let width = decoded.width() as usize;
    let height = decoded.height() as usize;

    let y_plane = copy_plane_from_av(decoded, 0);
    let u_plane = copy_plane_from_av(decoded, 1);
    let v_plane = copy_plane_from_av(decoded, 2);

    let mut data = Vec::with_capacity(width * height * 3 / 2);
    data.extend_from_slice(&y_plane);
    for (u, v) in u_plane.iter().zip(v_plane.iter()) {
        data.push(*u);
        data.push(*v);
    }

    let color_space = match decoded.color_space() {
        color::Space::BT709 => ColorSpace::BT709,
        color::Space::SMPTE170M => ColorSpace::BT601Ntsc,
        color::Space::BT470BG => ColorSpace::BT601Pal,
        _ => ColorSpace::Unspecified,
    };
    let color_range = match decoded.color_range() {
        color::Range::JPEG => ColorRange::Full,
        _ => ColorRange::Limited,
    };

    Nv12Frame {
        width,
        height,
        data,
        color_space,
        color_range,
    }
}

fn download_nv12_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    frame: OutputFrame<VideoTexture>,
) -> Nv12Frame {
    let texture = frame.data;
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
    let y_plane = texture.y_plane();
    encoder.copy_texture_to_buffer(
        y_plane.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(bytes_per_row as u32),
                rows_per_image: None,
            },
        },
        y_plane.size(),
    );
    let uv_plane = texture.uv_plane();
    encoder.copy_texture_to_buffer(
        uv_plane.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: y_plane_size,
                bytes_per_row: Some(bytes_per_row as u32),
                rows_per_image: None,
            },
        },
        uv_plane.size(),
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
        color_space: frame.metadata.color_space,
        color_range: frame.metadata.color_range,
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
