use std::{collections::VecDeque, ptr};

use compositor_render::{event_handler::emit_event, OutputId};
use crossbeam_channel::Receiver;
use ffmpeg_next::{self as ffmpeg, Rational, Rescale};
use tracing::{debug, error, trace, warn};

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
        sample_rate: u32,
        config: bytes::Bytes,
        audio_config: bytes::Bytes,
    ) -> Result<Self, OutputInitError> {
        let (output_ctx, video_stream, audio_stream) =
            init_ffmpeg_output(options, sample_rate, config, audio_config)?;

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
    encoder_config: bytes::Bytes,
    audio_encoder_config: bytes::Bytes,
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

    let mut video_stream = options
        .video
        .map(|v| {
            trace!("Init video track");
            const VIDEO_TIME_BASE: i32 = 1000;

            let mut stream = output_ctx
                .add_stream(ffmpeg::codec::Id::H264)
                .map_err(OutputInitError::FfmpegMp4Error)?;
            warn!("timebase video {}", stream.time_base());

           // stream.set_time_base(ffmpeg::Rational::new(1, 1000));

            let codecpar = unsafe { &mut *(*stream.as_mut_ptr()).codecpar };
            unsafe {
                // The allocated size of extradata must be at least extradata_size + AV_INPUT_BUFFER_PADDING_SIZE, with the padding bytes zeroed.
                codecpar.extradata = ffmpeg_next::ffi::av_mallocz(
                    encoder_config.len() + ffmpeg_next::ffi::AV_INPUT_BUFFER_PADDING_SIZE as usize,
                ) as *mut u8;
                std::ptr::copy(
                    encoder_config.as_ptr(),
                    codecpar.extradata,
                    encoder_config.len(),
                );
                codecpar.extradata_size = encoder_config.len() as i32;
            };
            codecpar.codec_id = ffmpeg::codec::Id::H264.into();
            codecpar.codec_type = ffmpeg::ffi::AVMediaType::AVMEDIA_TYPE_VIDEO;
            codecpar.width = v.width as i32;
            codecpar.height = v.height as i32;

            Ok::<Stream, OutputInitError>(Stream {
                id: stream.index(),
                time_base: stream.time_base(),
            })
        })
        .transpose()?;

    let mut audio_stream = options
        .audio
        .map(|a| {
            trace!("Init audio track");
            let channels = match a.channels {
                AudioChannels::Mono => 1,
                AudioChannels::Stereo => 2,
            };

            let mut stream = output_ctx
                .add_stream(ffmpeg::codec::Id::AAC)
                .map_err(OutputInitError::FfmpegMp4Error)?;

            warn!("timebase audio {} {}", stream.time_base(), sample_rate);
            // If audio time base doesn't match sample rate, ffmpeg muxer produces incorrect timestamps.
            //stream.set_time_base(ffmpeg::Rational::new(1, sample_rate as i32));

            let codecpar = unsafe { &mut *(*stream.as_mut_ptr()).codecpar };
            unsafe {
                // The allocated size of extradata must be at least extradata_size + AV_INPUT_BUFFER_PADDING_SIZE, with the padding bytes zeroed.
                codecpar.extradata = ffmpeg_next::ffi::av_mallocz(
                    encoder_config.len() + ffmpeg_next::ffi::AV_INPUT_BUFFER_PADDING_SIZE as usize,
                ) as *mut u8;
                std::ptr::copy(
                    audio_encoder_config.as_ptr(),
                    codecpar.extradata,
                    audio_encoder_config.len(),
                );
                codecpar.extradata_size = encoder_config.len() as i32;
            };
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

            Ok::<Stream, OutputInitError>(Stream {
                id: stream.index(),
                time_base: stream.time_base(),
            })
        })
        .transpose()?;

    output_ctx
        .write_header()
        .map_err(OutputInitError::FfmpegMp4Error)?;
    
    if let Some(video) = &mut video_stream {
        video.time_base = output_ctx.stream(0).unwrap().time_base();
    }
    if let Some(audio) = &mut audio_stream {
        audio.time_base = output_ctx.stream(1).unwrap().time_base();
    }

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
    let mut packet_buffer = EncodedChunkBuffer::new();

    for packet in packets_receiver {
        match packet {
            EncoderOutputEvent::Data(chunk) => {
                let ready_chunks = packet_buffer.next(chunk);
                for chunk in ready_chunks {
                    write_chunk(chunk, &video_stream, &audio_stream, &mut output_ctx);
                }
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
                Some(Stream { id, time_base }) => {
                    warn!("video, {:?} {:?} {:?}", chunk.pts, chunk.dts, time_base);
                    (*id, *time_base)
                }
                None => {
                    error!("Failed to create packet for video chunk. No video stream registered on init.");
                    return;
                }
            }
        }
        EncodedChunkKind::Audio(_) => {
            match audio_stream {
                Some(Stream { id, time_base }) => {
                    warn!("audio, {:?} {:?} {:?}", chunk.pts, chunk.dts, time_base);
                    (*id, *time_base)
                }
                None => {
                    error!("Failed to create packet for audio chunk. No audio stream registered on init.");
                    return;
                }
            }
        }
    };

    let mut packet = ffmpeg::Packet::copy(&chunk.data);
    packet.set_pts(Some(Rescale::rescale(
        &(chunk.pts.as_nanos() as i64),
        Rational(1, 1_000_000_000),
        time_base,
    )));
    let dts = chunk.dts.unwrap_or(chunk.pts);
    packet.set_dts(Some(Rescale::rescale(
        &(dts.as_nanos() as i64),
        Rational(1, 1_000_000_000),
        time_base,
    )));
    //packet.set_pts(Some((chunk.pts.as_secs_f64() * timebase) as i64));
    //packet.set_dts(Some((dts.as_secs_f64() * timebase) as i64));
    packet.set_time_base(time_base);
    if chunk.dts.is_none() {
        packet.set_duration(Rescale::rescale(
            &(20 as i64),
            Rational(1, 1_000),
            time_base,
        ));
    }
    packet.set_stream(stream_id);

    warn!(
        "dts - {:?} {:?} {:?} {:?}",
        &(dts.as_nanos() as i64),
        Rational(1, 1_000_000_000),
        time_base,
        packet.dts()
    );

    if let Err(err) = packet.write_interleaved(output_ctx) {
        error!("Failed to write packet to RTMP stream: {}.", err);
    }
}

#[derive(Debug, Clone)]
struct Stream {
    id: usize,
    time_base: Rational,
}

struct EncodedChunkBuffer {
    audio: VecDeque<EncodedChunk>,
    video: VecDeque<EncodedChunk>,
}

impl EncodedChunkBuffer {
    fn new() -> Self {
        return Self {
            audio: VecDeque::new(),
            video: VecDeque::new(),
        };
    }

    fn next(&mut self, chunk: EncodedChunk) -> Vec<EncodedChunk> {
        match chunk.kind {
            EncodedChunkKind::Video(_) => self.video.push_back(chunk),
            EncodedChunkKind::Audio(_) => self.audio.push_back(chunk),
        }
        let mut result = vec![];
        while let Some(chunk) = self.try_get() {
            result.push(chunk);
        }
        result
    }

    fn try_get(&mut self) -> Option<EncodedChunk> {
        match (self.audio.front(), self.video.front()) {
            (Some(audio), Some(video)) => {
                if audio.pts < video.dts.unwrap_or(video.pts) {
                    self.audio.pop_front()
                } else {
                    self.video.pop_front()
                }
            }
            _ => {
                if self.audio.len() > 1000 {
                    self.audio.pop_front()
                } else if self.video.len() > 1000 {
                    self.video.pop_front()
                } else {
                    None
                }
            }
        }
    }
}
