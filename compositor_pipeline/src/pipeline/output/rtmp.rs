use std::ptr;

use compositor_render::{event_handler::emit_event, OutputId};
use crossbeam_channel::Receiver;
use ffmpeg_next::{self as ffmpeg, Rational, Rescale};
use tracing::{debug, error};

use crate::{
    audio_mixer::AudioChannels,
    error::OutputInitError,
    event::Event,
    pipeline::{
        encoder::{
            AudioEncoderContext, AudioEncoderOptions, EncoderContext, VideoEncoderContext,
            VideoEncoderOptions,
        },
        types::IsKeyframe,
        EncodedChunk, EncodedChunkKind, EncoderOutputEvent,
    },
};

#[derive(Debug, Clone)]
pub struct RtmpSenderOptions {
    pub url: String,
    pub video: Option<VideoEncoderOptions>,
    pub audio: Option<AudioEncoderOptions>,
}

#[derive(Debug, Clone)]
struct Stream {
    index: usize,
    time_base: Rational,
}

pub struct RmtpSender;

impl RmtpSender {
    pub fn new(
        output_id: &OutputId,
        options: RtmpSenderOptions,
        packets_receiver: Receiver<EncoderOutputEvent>,
        encoder_ctx: EncoderContext,
    ) -> Result<Self, OutputInitError> {
        let (output_ctx, video_stream, audio_stream) = init_ffmpeg_output(options, encoder_ctx)?;

        let output_id = output_id.clone();
        std::thread::Builder::new()
            .name(format!("RTMP sender thread for output {}", output_id))
            .spawn(move || {
                let _span =
                    tracing::info_span!("RTMP sender  writer", output_id = output_id.to_string())
                        .entered();

                run_ffmpeg_output_thread(output_ctx, video_stream, audio_stream, packets_receiver);
                emit_event(Event::OutputDone(output_id));
                debug!("Closing RTMP sender thread.");
            })
            .unwrap();
        Ok(Self)
    }
}

fn init_ffmpeg_output(
    options: RtmpSenderOptions,
    encoder_ctx: EncoderContext,
) -> Result<
    (
        ffmpeg::format::context::Output,
        Option<Stream>,
        Option<Stream>,
    ),
    OutputInitError,
> {
    let mut output_ctx =
        ffmpeg::format::output_as(&options.url, "flv").map_err(OutputInitError::FfmpegError)?;

    let video = if let (Some(opts), Some(encoder_ctx)) = (&options.video, encoder_ctx.video) {
        let mut stream = output_ctx
            .add_stream(ffmpeg::codec::Id::H264)
            .map_err(OutputInitError::FfmpegError)?;

        let codecpar = unsafe { &mut *(*stream.as_mut_ptr()).codecpar };

        if let VideoEncoderContext::H264(Some(ctx)) = encoder_ctx {
            unsafe {
                // The allocated size of extradata must be at least extradata_size + AV_INPUT_BUFFER_PADDING_SIZE, with the padding bytes zeroed.
                codecpar.extradata = ffmpeg_next::ffi::av_mallocz(
                    ctx.len() + ffmpeg_next::ffi::AV_INPUT_BUFFER_PADDING_SIZE as usize,
                ) as *mut u8;
                std::ptr::copy(ctx.as_ptr(), codecpar.extradata, ctx.len());
                codecpar.extradata_size = ctx.len() as i32;
            };
        }

        codecpar.codec_id = ffmpeg::codec::Id::H264.into();
        codecpar.codec_type = ffmpeg::ffi::AVMediaType::AVMEDIA_TYPE_VIDEO;
        codecpar.width = opts.resolution().width as i32;
        codecpar.height = opts.resolution().height as i32;
        Some(stream.index())
    } else {
        None
    };

    let audio = if let (Some(opts), Some(encoder_ctx)) = (&options.audio, encoder_ctx.audio) {
        let channels = match opts.channels() {
            AudioChannels::Mono => 1,
            AudioChannels::Stereo => 2,
        };

        let mut stream = output_ctx
            .add_stream(ffmpeg::codec::Id::AAC)
            .map_err(OutputInitError::FfmpegError)?;

        let codecpar = unsafe { &mut *(*stream.as_mut_ptr()).codecpar };
        if let AudioEncoderContext::Aac(ctx) = encoder_ctx {
            unsafe {
                // The allocated size of extradata must be at least extradata_size + AV_INPUT_BUFFER_PADDING_SIZE, with the padding bytes zeroed.
                codecpar.extradata = ffmpeg_next::ffi::av_mallocz(
                    ctx.len() + ffmpeg_next::ffi::AV_INPUT_BUFFER_PADDING_SIZE as usize,
                ) as *mut u8;
                std::ptr::copy(ctx.as_ptr(), codecpar.extradata, ctx.len());
                codecpar.extradata_size = ctx.len() as i32;
            };
        }
        codecpar.codec_id = ffmpeg::codec::Id::AAC.into();
        codecpar.codec_type = ffmpeg::ffi::AVMediaType::AVMEDIA_TYPE_AUDIO;
        codecpar.sample_rate = opts.sample_rate() as i32;
        codecpar.ch_layout = ffmpeg::ffi::AVChannelLayout {
            nb_channels: channels,
            order: ffmpeg::ffi::AVChannelOrder::AV_CHANNEL_ORDER_UNSPEC,
            // This value is ignored when order is AV_CHANNEL_ORDER_UNSPEC
            u: ffmpeg::ffi::AVChannelLayout__bindgen_ty_1 { mask: 0 },
            // Field doc: "For some private data of the user."
            opaque: ptr::null_mut(),
        };
        Some(stream.index())
    } else {
        None
    };

    output_ctx
        .write_header()
        .map_err(OutputInitError::FfmpegError)?;

    let video = video.map(|index| Stream {
        index,
        time_base: output_ctx.stream(index).unwrap().time_base(),
    });
    let audio = audio.map(|index| Stream {
        index,
        time_base: output_ctx.stream(index).unwrap().time_base(),
    });

    Ok((output_ctx, video, audio))
}

fn run_ffmpeg_output_thread(
    mut output_ctx: ffmpeg::format::context::Output,
    video_stream: Option<Stream>,
    audio_stream: Option<Stream>,
    packets_receiver: Receiver<EncoderOutputEvent>,
) {
    let mut received_video_eos = video_stream.as_ref().map(|_| false);
    let mut received_audio_eos = audio_stream.as_ref().map(|_| false);

    for packet in packets_receiver {
        match packet {
            EncoderOutputEvent::Data(chunk) => {
                write_chunk(chunk, &video_stream, &audio_stream, &mut output_ctx);
            }
            EncoderOutputEvent::VideoEOS => match received_video_eos {
                Some(false) => received_video_eos = Some(true),
                Some(true) => {
                    error!("Received multiple video EOS events.");
                }
                None => {
                    error!("Received video EOS event on non video output.");
                }
            },
            EncoderOutputEvent::AudioEOS => match received_audio_eos {
                Some(false) => received_audio_eos = Some(true),
                Some(true) => {
                    error!("Received multiple audio EOS events.");
                }
                None => {
                    error!("Received audio EOS event on non audio output.");
                }
            },
        };

        if received_video_eos.unwrap_or(true) && received_audio_eos.unwrap_or(true) {
            if let Err(err) = output_ctx.write_trailer() {
                error!("Failed to write trailer to RTMP stream: {}.", err);
            };
            break;
        }
    }
}

fn write_chunk(
    chunk: EncodedChunk,
    video_stream: &Option<Stream>,
    audio_stream: &Option<Stream>,
    output_ctx: &mut ffmpeg::format::context::Output,
) {
    let (stream_id, time_base) = match chunk.kind {
        EncodedChunkKind::Video(_) => {
            match video_stream {
                Some(Stream { index, time_base }) => (*index, *time_base),
                None => {
                    error!("Failed to create packet for video chunk. No video stream registered on init.");
                    return;
                }
            }
        }
        EncodedChunkKind::Audio(_) => {
            match audio_stream {
                Some(Stream { index, time_base }) => (*index, *time_base),
                None => {
                    error!("Failed to create packet for audio chunk. No audio stream registered on init.");
                    return;
                }
            }
        }
    };

    const NS_TIME_BASE: Rational = Rational(1, 1_000_000_000);

    let mut packet = ffmpeg::Packet::copy(&chunk.data);
    packet.set_pts(Some(Rescale::rescale(
        &(chunk.pts.as_nanos() as i64),
        NS_TIME_BASE,
        time_base,
    )));

    let dts = chunk.dts.unwrap_or(chunk.pts);
    packet.set_dts(Some(Rescale::rescale(
        &(dts.as_nanos() as i64),
        NS_TIME_BASE,
        time_base,
    )));

    packet.set_time_base(time_base);
    packet.set_stream(stream_id);

    if let IsKeyframe::Yes = chunk.is_keyframe {
        packet.set_flags(ffmpeg_next::packet::Flags::KEY);
    }

    if let Err(err) = packet.write_interleaved(output_ctx) {
        error!("Failed to write packet to RTMP stream: {}.", err);
    }
}
