use std::{
    io::Read,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::SyncSender,
    },
    time::Duration,
};

use bytes::BytesMut;
use gpu_video::{EncodedInputChunk, OutputFrame, VideoDeviceExt, parameters::DecoderParameters};

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

    let receiver_gone = Arc::new(AtomicBool::new(false));
    let on_frame = {
        let receiver_gone = receiver_gone.clone();
        move |frame: OutputFrame<wgpu::Texture>| {
            let result = FrameWithPts {
                frame: frame.data,
                pts: Duration::from_secs_f64(frame_number as f64 * frame_interval),
            };

            frame_number += 1;

            if tx.send(result).is_err() {
                receiver_gone.store(true, Ordering::Relaxed);
            }
        }
    };

    let mut decoder = device
        .video()
        .unwrap()
        .create_wgpu_textures_decoder_h264(&queue, DecoderParameters::default(), on_frame)
        .unwrap();

    while let Ok(n) = bytestream_reader.read(&mut buffer) {
        if n == 0 || receiver_gone.load(Ordering::Relaxed) {
            break;
        }

        let frame = EncodedInputChunk {
            data: &buffer[..n],
            pts: None,
        };

        decoder.decode(frame).unwrap();
    }

    if !receiver_gone.load(Ordering::Relaxed) {
        decoder.flush().unwrap();
    }
}
