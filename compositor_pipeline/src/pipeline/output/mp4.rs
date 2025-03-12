use std::{fs, path::PathBuf, ptr, sync::Arc, time::Duration};

use compositor_render::OutputId;
use crossbeam_channel::Receiver;
use ffmpeg_next as ffmpeg;
use log::error;
use tracing::{debug, warn};

use crate::{
    audio_mixer::AudioChannels,
    error::OutputInitError,
    event::Event,
    pipeline::{
        types::IsKeyframe, EncodedChunk, EncodedChunkKind, EncoderOutputEvent, PipelineCtx,
        VideoCodec,
    },
};

#[derive(Debug, Clone)]
pub struct Mp4OutputOptions {
    pub output_path: PathBuf,
    pub video: Option<Mp4VideoTrack>,
    pub audio: Option<Mp4AudioTrack>,
}

#[derive(Debug, Clone)]
pub struct Mp4VideoTrack {
    pub codec: VideoCodec,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone)]
pub struct Mp4AudioTrack {
    pub channels: AudioChannels,
    pub sample_rate: u32,
}

pub enum Mp4OutputVideoTrack {
    H264 { width: u32, height: u32 },
}

pub struct Mp4WriterOptions {
    pub output_path: PathBuf,
    pub video: Option<Mp4OutputVideoTrack>,
}

pub struct Mp4FileWriter;

impl Mp4FileWriter {
    pub fn new(
        output_id: OutputId,
        options: Mp4OutputOptions,
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

        let (output_ctx, video_stream, audio_stream) = init_ffmpeg_output(options)?;

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

fn init_ffmpeg_output(
    options: Mp4OutputOptions,
) -> Result<
    (
        ffmpeg::format::context::Output,
        Option<StreamState>,
        Option<StreamState>,
    ),
    OutputInitError,
> {
    let mut output_ctx = ffmpeg::format::output_as(&options.output_path, "mp4")
        .map_err(OutputInitError::FfmpegMp4Error)?;

    let mut stream_count = 0;

    let video_stream = options
        .video
        .map(|v| {
            const VIDEO_TIME_BASE: i32 = 90000;

            let codec = match v.codec {
                VideoCodec::H264 => ffmpeg::codec::Id::H264,
                VideoCodec::VP8 => unreachable!(),
            };

            let mut stream = output_ctx
                .add_stream(ffmpeg::codec::Id::H264)
                .map_err(OutputInitError::FfmpegMp4Error)?;

            stream.set_time_base(ffmpeg::Rational::new(1, VIDEO_TIME_BASE));

            let codecpar = unsafe { &mut *(*stream.as_mut_ptr()).codecpar };
            codecpar.codec_id = codec.into();
            codecpar.codec_type = ffmpeg::ffi::AVMediaType::AVMEDIA_TYPE_VIDEO;
            codecpar.width = v.width as i32;
            codecpar.height = v.height as i32;

            let id = stream_count;
            stream_count += 1;

            Ok::<StreamState, OutputInitError>(StreamState {
                id,
                time_base: VIDEO_TIME_BASE as f64,
                timestamp_offset: None,
            })
        })
        .transpose()?;

    let audio_stream = options
        .audio
        .map(|a| {
            let codec = ffmpeg::codec::Id::AAC;
            let channels = match a.channels {
                AudioChannels::Mono => 1,
                AudioChannels::Stereo => 2,
            };
            let sample_rate = a.sample_rate as i32;

            let mut stream = output_ctx
                .add_stream(codec)
                .map_err(OutputInitError::FfmpegMp4Error)?;

            // If audio time base doesn't match sample rate, ffmpeg muxer produces incorrect timestamps.
            stream.set_time_base(ffmpeg::Rational::new(1, sample_rate));

            let codecpar = unsafe { &mut *(*stream.as_mut_ptr()).codecpar };
            codecpar.codec_id = codec.into();
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
                time_base: sample_rate as f64,
                timestamp_offset: None,
            })
        })
        .transpose()?;

    let ffmpeg_options = ffmpeg::Dictionary::from_iter(&[("movflags", "faststart")]);

    output_ctx
        .write_header_with(ffmpeg_options)
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
    let packet = create_packet(chunk, video_stream, audio_stream);
    if let Some(packet) = packet {
        if let Err(err) = packet.write(output_ctx) {
            error!("Failed to write packet to mp4 file: {}.", err);
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
            match video_stream {
                Some(stream_state) => Some(stream_state),
                None => {
                    error!("Failed to create packet for video chunk. No video stream registered on init.");
                    None
                }
            }
        }
        EncodedChunkKind::Audio(_) => {
            match audio_stream {
                Some(stream_state) => Some(stream_state),
                None => {
                    error!("Failed to create packet for audio chunk. No audio stream registered on init.");
                    None
                }
            }
        }
    }?;

    // Starting output PTS from 0
    let timestamp_offset = stream_state.timestamp_offset(&chunk);
    let pts = chunk.pts.saturating_sub(timestamp_offset);
    let dts = chunk
        .dts
        .map(|dts| dts.saturating_sub(timestamp_offset))
        .unwrap_or(pts);

    let mut packet = ffmpeg::Packet::copy(&chunk.data);
    packet.set_pts(Some((pts.as_secs_f64() * stream_state.time_base) as i64));
    packet.set_dts(Some((dts.as_secs_f64() * stream_state.time_base) as i64));
    packet.set_time_base(ffmpeg::Rational::new(1, stream_state.time_base as i32));
    packet.set_stream(stream_state.id);

    match chunk.is_keyframe {
        IsKeyframe::Yes => packet.set_flags(ffmpeg::packet::Flags::KEY),
        IsKeyframe::Unknown => warn!("The MP4 output received an encoded chunk with is_keyframe set to Unknown. This output needs this information to produce correct mp4s."),
        IsKeyframe::NoKeyframes | IsKeyframe::No => {},
    }

    Some(packet)
}

#[derive(Debug, Clone)]
struct StreamState {
    id: usize,
    time_base: f64,
    timestamp_offset: Option<Duration>,
}

impl StreamState {
    fn timestamp_offset(&mut self, chunk: &EncodedChunk) -> Duration {
        *self.timestamp_offset.get_or_insert(chunk.pts)
    }
}
