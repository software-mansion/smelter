use std::{fs, path::PathBuf, ptr, sync::Arc, time::Duration};

use compositor_render::OutputId;
use crossbeam_channel::Receiver;
use ffmpeg_next::{self as ffmpeg, Rational, Rescale};
use log::error;
use tracing::{debug, warn};

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
        EncodedChunk, EncodedChunkKind, EncoderOutputEvent, PipelineCtx,
    },
};

#[derive(Debug, Clone)]
pub struct Mp4OutputOptions {
    pub output_path: PathBuf,
    pub video: Option<VideoEncoderOptions>,
    pub audio: Option<AudioEncoderOptions>,
}

pub enum Mp4OutputVideoTrack {
    H264 { width: u32, height: u32 },
}

pub struct Mp4WriterOptions {
    pub output_path: PathBuf,
    pub video: Option<Mp4OutputVideoTrack>,
}

#[derive(Debug, Clone)]
struct StreamState {
    index: usize,
    time_base: Rational,
    timestamp_offset: Option<Duration>,
}

pub struct Mp4FileWriter;

impl Mp4FileWriter {
    pub fn new(
        output_id: OutputId,
        options: Mp4OutputOptions,
        encoder_ctx: EncoderContext,
        packets_receiver: Receiver<EncoderOutputEvent>,
        pipeline_ctx: Arc<PipelineCtx>,
    ) -> Result<Self, OutputInitError> {
        if options.output_path.exists() {
            let mut old_index = 0;
            let mut new_path_for_old_file;
            loop {
                new_path_for_old_file = PathBuf::from(format!(
                    "{}.old.{}",
                    options.output_path.to_string_lossy(),
                    old_index
                ));
                if !new_path_for_old_file.exists() {
                    break;
                }
                old_index += 1;
            }

            warn!(
                "Output file {} already exists. Renaming to {}.",
                options.output_path.to_string_lossy(),
                new_path_for_old_file.to_string_lossy()
            );
            if let Err(err) = fs::rename(options.output_path.clone(), new_path_for_old_file) {
                error!("Failed to rename existing output file. Error: {}", err);
            };
        }

        let (output_ctx, video_stream, audio_stream) = init_ffmpeg_output(options, encoder_ctx)?;

        let event_emitter = pipeline_ctx.event_emitter.clone();
        std::thread::Builder::new()
            .name(format!("MP4 writer thread for output {}", output_id))
            .spawn(move || {
                let _span =
                    tracing::info_span!("MP4 writer", output_id = output_id.to_string()).entered();

                run_ffmpeg_output_thread(output_ctx, video_stream, audio_stream, packets_receiver);
                event_emitter.emit(Event::OutputDone(output_id));
                debug!("Closing MP4 writer thread.");
            })
            .unwrap();

        Ok(Mp4FileWriter)
    }
}

const VIDEO_TIME_BASE: Rational = Rational(1, 90_000);
const NS_TIME_BASE: Rational = Rational(1, 1_000_000_000);

fn init_ffmpeg_output(
    options: Mp4OutputOptions,
    encoder_ctx: EncoderContext,
) -> Result<
    (
        ffmpeg::format::context::Output,
        Option<StreamState>,
        Option<StreamState>,
    ),
    OutputInitError,
> {
    let mut output_ctx = ffmpeg::format::output_as(&options.output_path, "mp4")
        .map_err(OutputInitError::FfmpegError)?;

    let video = if let (Some(opts), Some(encoder_ctx)) = (&options.video, encoder_ctx.video) {
        let codec = match opts {
            VideoEncoderOptions::H264(_) => ffmpeg::codec::Id::H264,
            VideoEncoderOptions::VP8(_) => unreachable!(),
        };

        let mut stream = output_ctx
            .add_stream(codec)
            .map_err(OutputInitError::FfmpegError)?;

        stream.set_time_base(VIDEO_TIME_BASE);

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

        codecpar.codec_id = codec.into();
        codecpar.codec_type = ffmpeg::ffi::AVMediaType::AVMEDIA_TYPE_VIDEO;
        codecpar.width = opts.resolution().width as i32;
        codecpar.height = opts.resolution().height as i32;

        Some(stream.index())
    } else {
        None
    };

    let audio = if let (Some(opts), Some(encoder_ctx)) = (&options.audio, encoder_ctx.audio) {
        let codec = ffmpeg::codec::Id::AAC;
        let channels = match opts.channels() {
            AudioChannels::Mono => 1,
            AudioChannels::Stereo => 2,
        };
        let sample_rate = opts.sample_rate() as i32;

        let mut stream = output_ctx
            .add_stream(codec)
            .map_err(OutputInitError::FfmpegError)?;

        // If audio time base doesn't match sample rate, ffmpeg muxer produces incorrect timestamps.
        stream.set_time_base(ffmpeg::Rational::new(1, sample_rate));

        let codecpar = unsafe { &mut *(*stream.as_mut_ptr()).codecpar };

        if let AudioEncoderContext::Aac(ctx) = encoder_ctx {
            unsafe {
                // The allocated size of extradata must be at least extradata_size + AV_INPUT_BUFFER_PADDING_SIZE, with the padding bytes zeroed.
                codecpar.extradata = ffmpeg_next::ffi::av_mallocz(
                    ctx.len() + ffmpeg_next::ffi::AV_INPUT_BUFFER_PADDING_SIZE as usize,
                ) as *mut u8;
                std::ptr::copy(ctx.as_ptr(), codecpar.extradata, ctx.len());
                codecpar.extradata_size = ctx.len() as i32;
            }
        }
        codecpar.codec_id = codec.into();
        codecpar.codec_type = ffmpeg::ffi::AVMediaType::AVMEDIA_TYPE_AUDIO;
        codecpar.sample_rate = sample_rate;
        codecpar.profile = ffmpeg::ffi::FF_PROFILE_AAC_LOW;
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

    let ffmpeg_options = ffmpeg::Dictionary::from_iter(&[("movflags", "faststart")]);

    output_ctx
        .write_header_with(ffmpeg_options)
        .map_err(OutputInitError::FfmpegError)?;

    let video = video.map(|index| StreamState {
        index,
        timestamp_offset: None,
        time_base: output_ctx.stream(index).unwrap().time_base(),
    });
    let audio = audio.map(|index| StreamState {
        index,
        timestamp_offset: None,
        time_base: output_ctx.stream(index).unwrap().time_base(),
    });

    Ok((output_ctx, video, audio))
}

fn run_ffmpeg_output_thread(
    mut output_ctx: ffmpeg::format::context::Output,
    mut video_stream: Option<StreamState>,
    mut audio_stream: Option<StreamState>,
    packets_receiver: Receiver<EncoderOutputEvent>,
) {
    let mut received_video_eos = video_stream.as_ref().map(|_| false);
    let mut received_audio_eos = audio_stream.as_ref().map(|_| false);

    for packet in packets_receiver {
        match packet {
            EncoderOutputEvent::Data(chunk) => {
                write_chunk(chunk, &mut video_stream, &mut audio_stream, &mut output_ctx);
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
                error!("Failed to write trailer to mp4 file: {}.", err);
            };
            break;
        }
    }
}

fn write_chunk(
    chunk: EncodedChunk,
    video_stream: &mut Option<StreamState>,
    audio_stream: &mut Option<StreamState>,
    output_ctx: &mut ffmpeg::format::context::Output,
) {
    let stream = match chunk.kind {
        EncodedChunkKind::Video(_) => {
            match video_stream {
                Some(stream) => stream,
                None => {
                    error!("Failed to create packet for video chunk. No video stream registered on init.");
                    return;
                }
            }
        }
        EncodedChunkKind::Audio(_) => {
            match audio_stream {
                Some(stream) => stream,
                None => {
                    error!("Failed to create packet for audio chunk. No audio stream registered on init.");
                    return;
                }
            }
        }
    };

    // Starting output PTS from 0
    let timestamp_offset = *stream.timestamp_offset.get_or_insert(chunk.pts);

    let pts = chunk.pts.saturating_sub(timestamp_offset);
    let dts = chunk
        .dts
        .map(|dts| dts.saturating_sub(timestamp_offset))
        .unwrap_or(pts);

    let mut packet = ffmpeg::Packet::copy(&chunk.data);
    packet.set_pts(Some(Rescale::rescale(
        &(pts.as_nanos() as i64),
        NS_TIME_BASE,
        stream.time_base,
    )));
    packet.set_dts(Some(Rescale::rescale(
        &(dts.as_nanos() as i64),
        NS_TIME_BASE,
        stream.time_base,
    )));
    packet.set_time_base(stream.time_base);
    packet.set_stream(stream.index);

    match chunk.is_keyframe {
        IsKeyframe::Yes => packet.set_flags(ffmpeg::packet::Flags::KEY),
        IsKeyframe::Unknown => warn!("The MP4 output received an encoded chunk with is_keyframe set to Unknown. This output needs this information to produce correct mp4s."),
        IsKeyframe::NoKeyframes | IsKeyframe::No => {},
    }

    if let Err(err) = packet.write(output_ctx) {
        error!("Failed to write packet to mp4 file: {}.", err);
    }
}
