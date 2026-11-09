use std::{collections::VecDeque, path::Path, sync::mpsc::Receiver, time::Duration};

use egui_wgpu::wgpu;
use gpu_video::{
    EncodedInputChunk, OutputFrame, VideoDeviceExt, VideoTexture, WgpuTexturesDecoderH264,
    parameters::{DecoderParameters, InputBytestreamFormatH264},
};

use crate::demuxer::Mp4Demuxer;

pub struct Playback {
    demuxer: Mp4Demuxer,
    decoder: WgpuTexturesDecoderH264,
    frame_receiver: Receiver<OutputFrame<VideoTexture>>,
    eof: bool,
    decoded_frames: VecDeque<OutputFrame<VideoTexture>>,
    pub frame_number: u64,
    pub time: Duration,
}

impl Playback {
    const MAX_DECODED_FRAMES: usize = 3;

    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, path: &Path) -> Self {
        let demuxer = Mp4Demuxer::new(path);
        let (frame_sender, frame_receiver) = std::sync::mpsc::channel();
        let parameters = DecoderParameters {
            bytestream_format: InputBytestreamFormatH264::Avcc {
                prefix_length: demuxer.prefix_length,
            },
            ..Default::default()
        };
        let decoder = device
            .video()
            .unwrap()
            .create_wgpu_textures_decoder_h264(queue, parameters, move |frame| {
                frame_sender.send(frame).unwrap();
            })
            .unwrap();

        Self {
            demuxer,
            decoder,
            frame_receiver,
            eof: false,
            decoded_frames: VecDeque::new(),
            frame_number: 0,
            time: Duration::ZERO,
        }
    }

    pub fn next_frame(&mut self, elapsed: Duration) -> Option<OutputFrame<VideoTexture>> {
        self.time += elapsed;

        while self.decoded_frames.len() < Self::MAX_DECODED_FRAMES {
            let Some(frame) = self.decode_next() else {
                break;
            };
            self.decoded_frames.push_back(frame);
        }

        let is_due = self
            .decoded_frames
            .front()
            .is_some_and(|f| Duration::from_micros(f.metadata.pts.unwrap()) <= self.time);
        if !is_due {
            return None;
        }
        self.frame_number += 1;
        self.decoded_frames.pop_front()
    }

    fn decode_next(&mut self) -> Option<OutputFrame<VideoTexture>> {
        loop {
            if let Ok(frame) = self.frame_receiver.try_recv() {
                return Some(frame);
            }

            if self.eof {
                return None;
            }

            match self.demuxer.next_chunk() {
                Some(chunk) => self
                    .decoder
                    .decode(EncodedInputChunk {
                        data: &chunk.data,
                        pts: Some(chunk.pts.as_micros() as u64),
                    })
                    .unwrap(),
                None => {
                    self.decoder.flush().unwrap();
                    self.eof = true;
                }
            }
        }
    }
}
