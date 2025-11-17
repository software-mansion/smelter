use ffmpeg_next::{Dictionary, Packet, Stream, format::context, media::Type};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use crate::pipeline::rtmp::rtmp_input::input_with_dictionary_and_interrupt;

pub(super) struct FfmpegInputContext {
    ctx: context::Input,
}

impl FfmpegInputContext {
    pub(super) fn new(
        url: &Arc<str>,
        should_close: Arc<AtomicBool>,
    ) -> Result<Self, ffmpeg_next::Error> {
        let ctx = input_with_dictionary_and_interrupt(
            url,
            Dictionary::from_iter([
                ("protocol_whitelist", "rtmp,rtmps,tcp,udp,crypto,file"),
                ("listen", "1"),
            ]),
            // move is required even though types do not require it
            move || should_close.load(Ordering::Relaxed),
        )?;
        Ok(Self { ctx })
    }

    pub(super) fn audio_stream(&self) -> Option<Stream<'_>> {
        self.ctx.streams().best(Type::Audio)
    }

    pub(super) fn video_stream(&self) -> Option<Stream<'_>> {
        self.ctx.streams().best(Type::Video)
    }

    pub(super) fn read_packet(&mut self) -> Result<Packet, ffmpeg_next::Error> {
        let mut packet = Packet::empty();
        packet.read(&mut self.ctx)?;
        Ok(packet)
    }
}
