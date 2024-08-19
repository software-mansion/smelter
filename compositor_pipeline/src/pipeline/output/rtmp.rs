use std::ptr;

use compositor_render::{event_handler::emit_event, OutputId};
use crossbeam_channel::Receiver;
use ffmpeg_next as ffmpeg;
use tracing::{debug, error};

use crate::{
    audio_mixer::AudioChannels,
    error::OutputInitError,
    event::Event,
    pipeline::{EncodedChunk, EncodedChunkKind, EncoderOutputEvent},
};

#[derive(Debug, Clone)]
pub struct RtmpSenderOptions {
    pub url: String,
    pub video: Option<RtmpVideoTrack>,
    pub audio: Option<RtmpAudioTrack>,
}

#[derive(Debug, Clone)]
pub struct RtmpVideoTrack {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone)]
pub struct RtmpAudioTrack {
    pub channels: AudioChannels,
}

pub struct RmtpSender;

impl RmtpSender {
    pub fn new(
        output_id: &OutputId,
        options: RtmpSenderOptions,
        packets_receiver: Receiver<EncoderOutputEvent>,
        sample_rate: u32,
    ) -> Result<Self, OutputInitError> {
        let (output_ctx, video_stream, audio_stream) = init_ffmpeg_output(options, sample_rate)?;

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
    sample_rate: u32,
) -> Result<
    (
        ffmpeg::format::context::Output,
        Option<Stream>,
        Option<Stream>,
    ),
    OutputInitError,
> {
    let mut output_ctx =
        ffmpeg::format::output_as(&options.url, "flv").map_err(OutputInitError::FfmpegMp4Error)?;

    let mut stream_count = 0;

    let video_stream = options
        .video
        .map(|v| {
            const VIDEO_TIME_BASE: i32 = 90000;

            let mut stream = output_ctx
                .add_stream(ffmpeg::codec::Id::H264)
                .map_err(OutputInitError::FfmpegMp4Error)?;

            stream.set_time_base(ffmpeg::Rational::new(1, VIDEO_TIME_BASE));

            let codecpar = unsafe { &mut *(*stream.as_mut_ptr()).codecpar };
            codecpar.codec_id = ffmpeg::codec::Id::H264.into();
            codecpar.codec_type = ffmpeg::ffi::AVMediaType::AVMEDIA_TYPE_VIDEO;
            codecpar.width = v.width as i32;
            codecpar.height = v.height as i32;

            let id = stream_count;
            stream_count += 1;

            Ok::<Stream, OutputInitError>(Stream {
                id,
                time_base: VIDEO_TIME_BASE as f64,
            })
        })
        .transpose()?;

    let audio_stream = options
        .audio
        .map(|a| {
            let channels = match a.channels {
                AudioChannels::Mono => 1,
                AudioChannels::Stereo => 2,
            };

            let mut stream = output_ctx
                .add_stream(ffmpeg::codec::Id::AAC)
                .map_err(OutputInitError::FfmpegMp4Error)?;

            // If audio time base doesn't match sample rate, ffmpeg muxer produces incorrect timestamps.
            stream.set_time_base(ffmpeg::Rational::new(1, sample_rate as i32));

            let codecpar = unsafe { &mut *(*stream.as_mut_ptr()).codecpar };
            codecpar.codec_id = ffmpeg::codec::Id::AAC.into();
            codecpar.codec_type = ffmpeg::ffi::AVMediaType::AVMEDIA_TYPE_AUDIO;
            codecpar.sample_rate = sample_rate as i32;
            codecpar.ch_layout = ffmpeg::ffi::AVChannelLayout {
                nb_channels: channels,
                order: ffmpeg::ffi::AVChannelOrder::AV_CHANNEL_ORDER_UNSPEC,
                // This value is ignored when order is AV_CHANNEL_ORDER_UNSPEC
                u: ffmpeg::ffi::AVChannelLayout__bindgen_ty_1 { mask: 0 },
                // Field doc: "For some private data of the user."
                opaque: ptr::null_mut(),
            };

            let id = stream_count;
            stream_count += 1;

            Ok::<Stream, OutputInitError>(Stream {
                id,
                time_base: sample_rate as f64,
            })
        })
        .transpose()?;

    output_ctx
        .write_header()
        .map_err(OutputInitError::FfmpegMp4Error)?;

    Ok((output_ctx, video_stream, audio_stream))
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
    let packet = create_packet(chunk, video_stream, audio_stream);
    if let Some(packet) = packet {
        if let Err(err) = packet.write(output_ctx) {
            error!("Failed to write packet to RTMP stream: {}.", err);
        }
    }
}

fn create_packet(
    chunk: EncodedChunk,
    video_stream: &Option<Stream>,
    audio_stream: &Option<Stream>,
) -> Option<ffmpeg::Packet> {
    let (stream_id, timebase) = match chunk.kind {
        EncodedChunkKind::Video(_) => {
            match video_stream {
                Some(Stream { id, time_base }) => Some((*id, *time_base)),
                None => {
                    error!("Failed to create packet for video chunk. No video stream registered on init.");
                    None
                }
            }
        }
        EncodedChunkKind::Audio(_) => {
            match audio_stream {
                Some(Stream { id, time_base }) => Some((*id, *time_base)),
                None => {
                    error!("Failed to create packet for audio chunk. No audio stream registered on init.");
                    None
                }
            }
        }
    }?;

    let mut packet = ffmpeg::Packet::copy(&chunk.data);
    packet.set_pts(Some((chunk.pts.as_secs_f64() * timebase) as i64));
    let dts = chunk.dts.unwrap_or(chunk.pts);
    packet.set_dts(Some((dts.as_secs_f64() * timebase) as i64));
    packet.set_time_base(ffmpeg::Rational::new(1, timebase as i32));
    packet.set_stream(stream_id);

    Some(packet)
}

#[derive(Debug, Clone)]
struct Stream {
    id: usize,
    time_base: f64,
}
