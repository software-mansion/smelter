use std::time::Duration;

use ffmpeg_next::ffi::{EAGAIN, EIO};
use tracing::{trace, warn};

use crate::pipeline::rtmp::rtmp_input::{Track, ffmpeg_context::FfmpegInputContext};

use crate::prelude::*;

const RTMP_READ_RETRY_DELAY: Duration = Duration::from_millis(10);

pub(super) fn run_demuxer_loop(
    mut input_ctx: FfmpegInputContext,
    mut audio: Option<Track>,
    mut video: Option<Track>,
) {
    loop {
        let packet = match input_ctx.read_packet() {
            Ok(packet) => packet,
            Err(ffmpeg_next::Error::Eof | ffmpeg_next::Error::Exit) => break,
            Err(ffmpeg_next::Error::Other { errno: EAGAIN }) => {
                trace!("RTMP demuxer waiting for packets");
                std::thread::sleep(RTMP_READ_RETRY_DELAY);
                continue;
            }
            Err(ffmpeg_next::Error::Other { errno: EIO }) => {
                warn!("Input session disconnected!");
                break;
            }
            Err(err) => {
                trace!("RTMP read error {err:?}");
                continue;
            }
        };

        if let Some(track) = &mut video
            && packet.stream() == track.index
        {
            track.send_packet(&packet, MediaKind::Video(VideoCodec::H264));
        }

        if let Some(track) = &mut audio
            && packet.stream() == track.index
        {
            track.send_packet(&packet, MediaKind::Audio(AudioCodec::Aac));
        }
    }
}
