use crossbeam_channel::Receiver;
use gpu_video::{
    BytesEncoderH264, BytesEncoderH265, EncodedOutputChunk, InputFrame, RawFrameRef,
    VideoDeviceExt, WgpuTexturesEncoderH264, WgpuTexturesEncoderH265,
    parameters::{EncoderParametersH264, EncoderParametersH265},
};

use crate::gpu_video_tests::Nv12Frame;

#[derive(Debug, Clone, Copy)]
pub enum EncoderParameters {
    H264(EncoderParametersH264),
    H265(EncoderParametersH265),
}

pub(super) trait Encoder {
    fn encoder_name(&self) -> &'static str;

    fn encode(
        &mut self,
        frame: &Nv12Frame,
        pts: u64,
        force_keyframe: bool,
    ) -> Vec<EncodedOutputChunk<Vec<u8>>>;
    fn flush(&mut self) -> Vec<EncodedOutputChunk<Vec<u8>>>;
    fn stream_params(&self) -> Vec<u8>;
}

enum BytesEncoderKind {
    H264(BytesEncoderH264),
    H265(BytesEncoderH265),
}

pub struct GvBytesEncoder {
    encoder: BytesEncoderKind,
    chunk_receiver: Receiver<EncodedOutputChunk<Vec<u8>>>,
}

impl GvBytesEncoder {
    pub fn new(wgpu_device: &wgpu::Device, parameters: EncoderParameters) -> Self {
        let (chunk_sender, chunk_receiver) = crossbeam_channel::unbounded();
        let video_device = wgpu_device.video().unwrap();
        let on_chunk = move |chunk| chunk_sender.send(chunk).unwrap();
        let encoder = match parameters {
            EncoderParameters::H264(parameters) => BytesEncoderKind::H264(
                video_device
                    .create_bytes_encoder_h264(parameters, on_chunk)
                    .unwrap(),
            ),
            EncoderParameters::H265(parameters) => BytesEncoderKind::H265(
                video_device
                    .create_bytes_encoder_h265(parameters, on_chunk)
                    .unwrap(),
            ),
        };

        Self {
            encoder,
            chunk_receiver,
        }
    }

    pub fn encode_raw(
        &mut self,
        frame: &InputFrame<RawFrameRef<'_>>,
        force_keyframe: bool,
    ) -> Result<(), gpu_video::VideoEncoderError> {
        match &mut self.encoder {
            BytesEncoderKind::H264(encoder) => encoder.encode(frame, force_keyframe),
            BytesEncoderKind::H265(encoder) => encoder.encode(frame, force_keyframe),
        }
    }
}

impl Encoder for GvBytesEncoder {
    fn encoder_name(&self) -> &'static str {
        match self.encoder {
            BytesEncoderKind::H264(_) => "BytesEncoderH264",
            BytesEncoderKind::H265(_) => "BytesEncoderH265",
        }
    }

    fn encode(
        &mut self,
        frame: &Nv12Frame,
        pts: u64,
        force_keyframe: bool,
    ) -> Vec<EncodedOutputChunk<Vec<u8>>> {
        let frame = InputFrame {
            data: RawFrameRef {
                frame: &frame.data,
                width: frame.width as u32,
                height: frame.height as u32,
            },
            pts: Some(pts),
        };
        self.encode_raw(&frame, force_keyframe).unwrap();
        self.chunk_receiver.try_iter().collect()
    }

    fn flush(&mut self) -> Vec<EncodedOutputChunk<Vec<u8>>> {
        match &mut self.encoder {
            BytesEncoderKind::H264(encoder) => encoder.flush().unwrap(),
            BytesEncoderKind::H265(encoder) => encoder.flush().unwrap(),
        }
        self.chunk_receiver.try_iter().collect()
    }

    fn stream_params(&self) -> Vec<u8> {
        match &self.encoder {
            BytesEncoderKind::H264(encoder) => {
                [encoder.sps().unwrap(), encoder.pps().unwrap()].concat()
            }
            BytesEncoderKind::H265(encoder) => [
                encoder.vps().unwrap(),
                encoder.sps().unwrap(),
                encoder.pps().unwrap(),
            ]
            .concat(),
        }
    }
}

enum WgpuTexturesEncoderKind {
    H264(WgpuTexturesEncoderH264),
    H265(WgpuTexturesEncoderH265),
}

pub(super) struct GvWgpuTexturesEncoder {
    encoder: WgpuTexturesEncoderKind,
    chunk_receiver: Receiver<EncodedOutputChunk<Vec<u8>>>,
    wgpu_queue: wgpu::Queue,
}

impl GvWgpuTexturesEncoder {
    pub fn new(
        wgpu_device: &wgpu::Device,
        wgpu_queue: &wgpu::Queue,
        parameters: EncoderParameters,
    ) -> Self {
        let (chunk_sender, chunk_receiver) = crossbeam_channel::unbounded();
        let video_device = wgpu_device.video().unwrap();
        let on_chunk = move |chunk| chunk_sender.send(chunk).unwrap();
        let encoder = match parameters {
            EncoderParameters::H264(parameters) => WgpuTexturesEncoderKind::H264(
                video_device
                    .create_wgpu_textures_encoder_h264(wgpu_queue, parameters, on_chunk)
                    .unwrap(),
            ),
            EncoderParameters::H265(parameters) => WgpuTexturesEncoderKind::H265(
                video_device
                    .create_wgpu_textures_encoder_h265(wgpu_queue, parameters, on_chunk)
                    .unwrap(),
            ),
        };

        Self {
            encoder,
            chunk_receiver,
            wgpu_queue: wgpu_queue.clone(),
        }
    }
}

impl Encoder for GvWgpuTexturesEncoder {
    fn encoder_name(&self) -> &'static str {
        match self.encoder {
            WgpuTexturesEncoderKind::H264(_) => "WgpuTexturesEncoderH264",
            WgpuTexturesEncoderKind::H265(_) => "WgpuTexturesEncoderH265",
        }
    }

    fn encode(
        &mut self,
        frame: &Nv12Frame,
        pts: u64,
        force_keyframe: bool,
    ) -> Vec<EncodedOutputChunk<Vec<u8>>> {
        let texture = match &mut self.encoder {
            WgpuTexturesEncoderKind::H264(encoder) => encoder.input_texture().unwrap(),
            WgpuTexturesEncoderKind::H265(encoder) => encoder.input_texture().unwrap(),
        };
        upload_nv12_frame(&self.wgpu_queue, texture.texture(), frame);

        let frame = InputFrame {
            data: texture,
            pts: Some(pts),
        };
        match &mut self.encoder {
            WgpuTexturesEncoderKind::H264(encoder) => encoder.encode(frame, force_keyframe),
            WgpuTexturesEncoderKind::H265(encoder) => encoder.encode(frame, force_keyframe),
        }
        .unwrap();
        self.chunk_receiver.try_iter().collect()
    }

    fn flush(&mut self) -> Vec<EncodedOutputChunk<Vec<u8>>> {
        match &mut self.encoder {
            WgpuTexturesEncoderKind::H264(encoder) => encoder.flush().unwrap(),
            WgpuTexturesEncoderKind::H265(encoder) => encoder.flush().unwrap(),
        }
        self.chunk_receiver.try_iter().collect()
    }

    fn stream_params(&self) -> Vec<u8> {
        match &self.encoder {
            WgpuTexturesEncoderKind::H264(encoder) => {
                [encoder.sps().unwrap(), encoder.pps().unwrap()].concat()
            }
            WgpuTexturesEncoderKind::H265(encoder) => [
                encoder.vps().unwrap(),
                encoder.sps().unwrap(),
                encoder.pps().unwrap(),
            ]
            .concat(),
        }
    }
}

fn upload_nv12_frame(queue: &wgpu::Queue, texture: &gpu_video::VideoTexture, frame: &Nv12Frame) {
    let y_plane_size = frame.width * frame.height;
    let y_plane = texture.y_plane();
    queue.write_texture(
        y_plane.as_image_copy(),
        &frame.data[..y_plane_size],
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(frame.width as u32),
            rows_per_image: None,
        },
        y_plane.size(),
    );
    let uv_plane = texture.uv_plane();
    queue.write_texture(
        uv_plane.as_image_copy(),
        &frame.data[y_plane_size..],
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(frame.width as u32),
            rows_per_image: None,
        },
        uv_plane.size(),
    );
    queue.submit([]);
}
