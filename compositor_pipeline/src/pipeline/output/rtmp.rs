use std::{ptr, time::Duration};

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
    pub sample_rate: u32,
}

pub struct RmtpSender;

impl RmtpSender {
    pub fn new(
        output_id: &OutputId,
        options: RtmpSenderOptions,
        packets_receiver: Receiver<EncoderOutputEvent>,
    ) -> Result<Self, OutputInitError> {
        let (output_ctx, video_stream, audio_stream) = init_ffmpeg_output(options)?;

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
) -> Result<
    (
        ffmpeg::format::context::Output,
        Option<StreamState>,
        Option<StreamState>,
    ),
    OutputInitError,
> {
    let mut output_ctx =
        ffmpeg::format::output_as(&options.url, "flv").map_err(OutputInitError::FfmpegMp4Error)?;

    let mut stream_count = 0;

    let video_stream = options
        .video
        .map(|v| {
            let mut stream = output_ctx
                .add_stream(ffmpeg::codec::Id::H264)
                .map_err(OutputInitError::FfmpegMp4Error)?;

            let codecpar = unsafe { &mut *(*stream.as_mut_ptr()).codecpar };
            codecpar.codec_id = ffmpeg::codec::Id::H264.into();
            codecpar.codec_type = ffmpeg::ffi::AVMediaType::AVMEDIA_TYPE_VIDEO;
            codecpar.width = v.width as i32;
            codecpar.height = v.height as i32;

            let id = stream_count;
            stream_count += 1;

            Ok::<StreamState, OutputInitError>(StreamState {
                id,
                timestamp_offset: None,
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
            let sample_rate = a.sample_rate as i32;

            let mut stream = output_ctx
                .add_stream(ffmpeg::codec::Id::AAC)
                .map_err(OutputInitError::FfmpegMp4Error)?;

            // If audio time base doesn't match sample rate, ffmpeg muxer produces incorrect timestamps.
            stream.set_time_base(ffmpeg::Rational::new(1, sample_rate));

            let codecpar = unsafe { &mut *(*stream.as_mut_ptr()).codecpar };
            codecpar.codec_id = ffmpeg::codec::Id::AAC.into();
            codecpar.codec_type = ffmpeg::ffi::AVMediaType::AVMEDIA_TYPE_AUDIO;
            codecpar.sample_rate = sample_rate;
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

            Ok::<StreamState, OutputInitError>(StreamState {
                id,
                timestamp_offset: None,
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
                error!("Failed to write trailer to RTMP stream: {}.", err);
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
    let packet = create_packet(chunk, video_stream, audio_stream);
    if let Some(packet) = packet {
        if let Err(err) = packet.write(output_ctx) {
            error!("Failed to write packet to RTMP stream: {}.", err);
        }
    }
}

fn create_packet(
    chunk: EncodedChunk,
    video_stream: &mut Option<StreamState>,
    audio_stream: &mut Option<StreamState>,
) -> Option<ffmpeg::Packet> {
    let stream_state = match chunk.kind {
        EncodedChunkKind::Video(_) => {
            match video_stream.as_mut() {
                Some(stream_state) => Some(stream_state),
                None => {
                    error!("Failed to create packet for video chunk. No video stream registered on init.");
                    None
                }
            }
        }
        EncodedChunkKind::Audio(_) => {
            match audio_stream.as_mut() {
                Some(stream_state) => Some(stream_state),
                None => {
                    error!("Failed to create packet for audio chunk. No audio stream registered on init.");
                    None
                }
            }
        }
    }?;
    let timestamp_offset = stream_state.timestamp_offset(&chunk);
    let pts = chunk.pts - timestamp_offset;
    // let dts = chunk.dts.map(|dts| dts - timestamp_offset).unwrap_or(pts);
    let dts = chunk.dts.map(|dts| dts - timestamp_offset);

    let mut packet = ffmpeg::Packet::copy(&chunk.data);
    packet.set_pts(Some((pts.as_secs_f64() * 1000.0) as i64));
    packet.set_dts(dts.map(|dts| (dts.as_secs_f64() * 1000.0) as i64));
    packet.set_time_base(ffmpeg::Rational::new(1, 1000));
    packet.set_stream(stream_state.id);

    Some(packet)
}

#[derive(Debug, Clone)]
struct StreamState {
    id: usize,
    timestamp_offset: Option<Duration>,
}

impl StreamState {
    fn timestamp_offset(&mut self, chunk: &EncodedChunk) -> Duration {
        *self.timestamp_offset.get_or_insert(chunk.pts)
    }
}
