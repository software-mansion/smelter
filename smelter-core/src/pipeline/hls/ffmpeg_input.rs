use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use bytes::Bytes;
use ffmpeg_next::{
    Dictionary, Packet, Rational, Stream, codec,
    ffi::AVDiscard,
    format::{context, input_with_interrupt_and_dictionary},
    media::Type,
};
use tracing::warn;

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
    /// Probing on open reads from all of them, after that only the selected streams are kept.
    /// Playlists of the discarded streams stop downloading on their next read.
    pub(super) fn new(
        url: &Arc<str>,
        should_close: Arc<AtomicBool>,
    ) -> Result<Self, ffmpeg_next::Error> {
        let mut ctx = input_with_interrupt_and_dictionary(
            &**url,
            move || should_close.load(Ordering::Relaxed),
            Dictionary::from_iter([("protocol_whitelist", "tcp,hls,http,https,file,tls")]),
        )?;

        let (video_index, audio_index) = {
            let streams = ctx.streams();
            let video = streams.best(Type::Video);
            // Audio from the variant of the video, so only one variant is downloaded.
            let audio = match &video {
                Some(video) => streams.related(video).best(Type::Audio),
                None => streams.best(Type::Audio),
            };
            (
                stream_with_codec(video, codec::Id::H264),
                stream_with_codec(audio, codec::Id::AAC),
            )
        };
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

/// Index of the stream if it has the codec we can decode.
fn stream_with_codec(stream: Option<Stream>, codec: codec::Id) -> Option<usize> {
    let stream = stream?;
    let stream_codec = stream.parameters().id();
    if stream_codec != codec {
        warn!(
            ?stream_codec,
            "Unsupported codec in HLS input, ignoring the track"
        );
        return None;
    }
    Some(stream.index())
}
