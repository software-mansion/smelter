use std::{
    sync::mpsc::{self, Receiver},
    time::Duration,
};

use egui_wgpu::wgpu;
use gpu_video::{
    EncodedInputChunk, OutputFrame, VideoDeviceExt, VideoTexture, WgpuTexturesDecoderH264,
    parameters::DecoderParameters,
};

use crate::demuxer::Chunk;

pub struct FrameWithPts {
    pub frame: VideoTexture,
    pub pts: Duration,
}

pub struct Decoder {
    decoder: WgpuTexturesDecoderH264,
    frame_receiver: Receiver<OutputFrame<VideoTexture>>,
}

impl Decoder {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let (tx, frame_receiver) = std::sync::mpsc::channel();
        let decoder = device
            .video()
            .unwrap()
            .create_wgpu_textures_decoder_h264(queue, DecoderParameters::default(), move |frame| {
                tx.send(frame).unwrap();
            })
            .unwrap();

        Self {
            decoder,
            frame_receiver,
        }
    }

    pub fn decode(&mut self, chunk: Chunk) {
        let chunk = EncodedInputChunk {
            data: &chunk.data,
            pts: Some(chunk.pts.as_micros() as u64),
        };
        self.decoder.decode(chunk).unwrap();
    }

    pub fn flush(&mut self) {
        self.decoder.flush().unwrap();
    }

    pub fn try_frame(&self) -> Option<FrameWithPts> {
        let frame = self.frame_receiver.try_recv().ok()?;
        Some(FrameWithPts {
            frame: frame.data,
            pts: Duration::from_micros(frame.metadata.pts.unwrap()),
        })
    }
}
