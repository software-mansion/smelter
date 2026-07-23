use std::{io::Read, sync::mpsc::SyncSender, time::Duration};

use bytes::BytesMut;
use gpu_video::{
    EncodedInputChunk, OutputFrame, VideoDecoderError, VideoDeviceExt,
    parameters::DecoderParameters,
};

use super::FrameWithPts;

pub fn run_decoder(
    tx: SyncSender<super::FrameWithPts>,
    framerate: u64,
    device: wgpu::Device,
    queue: wgpu::Queue,
    mut bytestream_reader: impl Read,
) {
    let frame_interval = 1.0 / (framerate as f64);
    let mut frame_number = 0u64;
    let mut buffer = BytesMut::zeroed(4096);

    let on_frame = move |frame: Result<OutputFrame<wgpu::Texture>, VideoDecoderError>| {
        let Ok(frame) = frame else {
            return;
        };

        let result = FrameWithPts {
            frame: frame.data,
            pts: Duration::from_secs_f64(frame_number as f64 * frame_interval),
        };

        frame_number += 1;

        let _ = tx.send(result);
    };

    let mut decoder = device
        .video()
        .unwrap()
        .create_wgpu_textures_decoder_h264(&queue, DecoderParameters::default(), on_frame)
        .unwrap();

    while let Ok(n) = bytestream_reader.read(&mut buffer) {
        if n == 0 {
            break;
        }

        let frame = EncodedInputChunk {
            data: &buffer[..n],
            pts: None,
        };

        decoder.decode(frame).unwrap();
    }

    decoder.flush().unwrap();
}
