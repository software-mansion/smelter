use std::{
    ffi::CString,
    ptr, slice,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use bytes::Bytes;
use ffmpeg_next::{
    Dictionary, DictionaryRef, Packet, Rational, codec,
    ffi::{
        AVDiscard, AVProgram, avformat_alloc_context, avformat_find_stream_info,
        avformat_open_input,
    },
    format::{context, stream::Disposition},
    util::interrupt,
};
use tracing::debug;

use crate::pipeline::ffmpeg_utils::ReadExtradataExt;

/// One track of the playlist, as the demuxer described it.
pub(super) struct HlsStream {
    pub(super) index: usize,
    pub(super) time_base: Rational,
    pub(super) extradata: Option<Bytes>,
}

pub(super) struct FfmpegInputContext {
    ctx: context::Input,
    video_index: Option<usize>,
    audio_index: Option<usize>,
}

impl FfmpegInputContext {
    /// FFmpeg downloads every variant of a master playlist unless its streams are discarded.
    /// Only one variant is kept, the rest are discarded before probing for stream info, so only
    /// opening the input touches them.
    pub(super) fn new(
        url: &Arc<str>,
        should_close: Arc<AtomicBool>,
    ) -> Result<Self, ffmpeg_next::Error> {
        let mut ctx = open_input(url, should_close)?;
        let variant_streams = select_variant(&mut ctx);

        let result = unsafe { avformat_find_stream_info(ctx.as_mut_ptr(), ptr::null_mut()) };
        if result < 0 {
            return Err(ffmpeg_next::Error::from(result));
        }

        let video_index = find_stream(&ctx, &variant_streams, codec::Id::H264);
        let audio_index = find_stream(&ctx, &variant_streams, codec::Id::AAC);
        // e.g. alternative audio renditions of the selected variant
        for mut stream in ctx.streams_mut() {
            let index = Some(stream.index());
            if index != video_index && index != audio_index {
                unsafe { (*stream.as_mut_ptr()).discard = AVDiscard::AVDISCARD_ALL };
            }
        }

        Ok(Self {
            ctx,
            video_index,
            audio_index,
        })
    }

    pub(super) fn audio_stream(&self) -> Option<HlsStream> {
        self.stream(self.audio_index?)
    }

    pub(super) fn video_stream(&self) -> Option<HlsStream> {
        self.stream(self.video_index?)
    }

    fn stream(&self, index: usize) -> Option<HlsStream> {
        let stream = self.ctx.stream(index)?;
        Some(HlsStream {
            index: stream.index(),
            time_base: stream.time_base(),
            extradata: stream.read_extradata(),
        })
    }

    /// The HLS demuxer only knows the duration of a playlist that ended
    /// (`#EXT-X-ENDLIST`), so a playlist without one is a live one.
    pub(super) fn is_live(&self) -> bool {
        self.ctx.duration() <= 0
    }

    pub(super) fn read_packet(&mut self) -> Result<Packet, ffmpeg_next::Error> {
        let mut packet = Packet::empty();
        packet.read(&mut self.ctx)?;
        Ok(packet)
    }
}

/// `avformat_open_input` without `avformat_find_stream_info`, so streams can be discarded before
/// probing reads from them.
fn open_input(
    url: &str,
    should_close: Arc<AtomicBool>,
) -> Result<context::Input, ffmpeg_next::Error> {
    let url = CString::new(url).map_err(|_| ffmpeg_next::Error::InvalidData)?;
    let interrupt = interrupt::new(Box::new(move || should_close.load(Ordering::Relaxed)));
    let opts = Dictionary::from_iter([("protocol_whitelist", "tcp,hls,http,https,file,tls")]);
    unsafe {
        let mut ctx = avformat_alloc_context();
        if ctx.is_null() {
            return Err(ffmpeg_next::Error::Other {
                errno: ffmpeg_next::error::ENOMEM,
            });
        }
        (*ctx).interrupt_callback = interrupt.interrupt;

        let mut opts = opts.disown();
        // frees `ctx` on failure
        let result = avformat_open_input(&mut ctx, url.as_ptr(), ptr::null(), &mut opts);
        Dictionary::own(opts);
        if result < 0 {
            return Err(ffmpeg_next::Error::from(result));
        }
        Ok(context::Input::wrap_with_interrupt(ctx, interrupt.guard))
    }
}

/// FFmpeg exposes each variant as a program. Picks the variant with the highest bandwidth,
/// preferring the ones with codecs we can decode, and discards the other programs. Returns
/// streams of the picked variant, or all streams if there are no programs.
fn select_variant(ctx: &mut context::Input) -> Vec<usize> {
    let programs = unsafe {
        let ctx = ctx.as_mut_ptr();
        match (*ctx).nb_programs {
            0 => &mut [],
            len => slice::from_raw_parts_mut((*ctx).programs, len as usize),
        }
    };
    let program_streams = |program: *mut AVProgram| -> Vec<usize> {
        unsafe {
            match (*program).nb_stream_indexes {
                0 => Vec::new(),
                len => slice::from_raw_parts((*program).stream_index, len as usize)
                    .iter()
                    .map(|&index| index as usize)
                    .collect(),
            }
        }
    };
    let has_codec = |streams: &[usize], codec: codec::Id| {
        streams.iter().any(|&index| {
            ctx.stream(index)
                .is_some_and(|stream| stream.parameters().id() == codec)
        })
    };
    let bandwidth = |program: *mut AVProgram| -> i64 {
        unsafe { DictionaryRef::wrap((*program).metadata) }
            .get("variant_bitrate")
            .and_then(|bitrate| bitrate.parse().ok())
            .unwrap_or(0)
    };

    let selected = programs
        .iter()
        .copied()
        .filter(|&program| unsafe { (*program).nb_stream_indexes } > 0)
        .max_by_key(|&program| {
            let streams = program_streams(program);
            (
                has_codec(&streams, codec::Id::H264),
                has_codec(&streams, codec::Id::AAC),
                bandwidth(program),
            )
        });
    let Some(selected) = selected else {
        return ctx.streams().map(|stream| stream.index()).collect();
    };

    debug!(
        variants = programs.len(),
        bandwidth = bandwidth(selected),
        "Selected HLS variant"
    );
    for &program in programs.iter() {
        if program != selected {
            unsafe { (*program).discard = AVDiscard::AVDISCARD_ALL };
        }
    }
    program_streams(selected)
}

/// Stream of the codec among `candidates`, the one marked as default if there are several.
fn find_stream(ctx: &context::Input, candidates: &[usize], codec: codec::Id) -> Option<usize> {
    candidates
        .iter()
        .filter_map(|&index| ctx.stream(index))
        .filter(|stream| stream.parameters().id() == codec)
        .min_by_key(|stream| !stream.disposition().contains(Disposition::DEFAULT))
        .map(|stream| stream.index())
}
