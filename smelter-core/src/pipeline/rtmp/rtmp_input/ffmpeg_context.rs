use std::{
    ffi::CString,
    ptr,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use ffmpeg_next::{
    Dictionary, Packet, Stream,
    ffi::{
        avformat_alloc_context, avformat_close_input, avformat_find_stream_info,
        avformat_open_input,
    },
    format::context,
    media::Type,
    util::interrupt,
};

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

/// Combined implementation of ffmpeg_next::format:input_with_interrupt and
/// ffmpeg_next::format::input_with_dictionary that allows passing both interrupt
/// callback and Dictionary with options
fn input_with_dictionary_and_interrupt<F>(
    path: &str,
    options: Dictionary,
    interrupt_fn: F,
) -> Result<context::Input, ffmpeg_next::Error>
where
    F: FnMut() -> bool + 'static,
{
    unsafe {
        let mut ps = avformat_alloc_context();

        (*ps).interrupt_callback = interrupt::new(Box::new(interrupt_fn)).interrupt;

        let path = CString::new(path).unwrap();
        let mut opts = options.disown();
        let res = avformat_open_input(&mut ps, path.as_ptr(), ptr::null_mut(), &mut opts);

        Dictionary::own(opts);

        match res {
            0 => match avformat_find_stream_info(ps, ptr::null_mut()) {
                r if r >= 0 => Ok(context::Input::wrap(ps)),
                e => {
                    avformat_close_input(&mut ps);
                    Err(ffmpeg_next::Error::from(e))
                }
            },

            e => Err(ffmpeg_next::Error::from(e)),
        }
    }
}
