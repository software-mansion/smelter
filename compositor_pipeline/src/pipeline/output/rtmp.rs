use std::{ptr, sync::Arc};

use crossbeam_channel::{bounded, Receiver, Sender};
use ffmpeg_next::{self as ffmpeg, Rational, Rescale};
use tracing::{debug, error};

use crate::prelude::*;
use crate::{
    event::Event,
    pipeline::encoder::{
        encoder_thread_audio::{spawn_audio_encoder_thread, AudioEncoderThreadHandle},
        encoder_thread_video::{spawn_video_encoder_thread, VideoEncoderThreadHandle},
        fdk_aac::FdkAacEncoder,
        ffmpeg_h264::FfmpegH264Encoder,
    },
};

use super::{Output, OutputAudio, OutputVideo};

#[derive(Debug, Clone)]
struct Stream {
    index: usize,
    time_base: Rational,
}

pub struct RtmpClientOutput {
    video: Option<VideoEncoderThreadHandle>,
    audio: Option<AudioEncoderThreadHandle>,
}

impl RtmpClientOutput {
    pub fn new(
        ctx: Arc<PipelineCtx>,
        output_id: OutputId,
        options: RtmpSenderOptions,
    ) -> Result<Self, OutputInitError> {
        let mut output_ctx =
            ffmpeg::format::output_as(&options.url, "flv").map_err(OutputInitError::FfmpegError)?;

        let (encoded_chunks_sender, encoded_chunks_receiver) = bounded(1);

        let video = match options.video {
            Some(video) => Some(Self::init_video_track(
                &ctx,
                &output_id,
                video,
                &mut output_ctx,
                encoded_chunks_sender.clone(),
            )?),
            None => None,
        };
        let audio = match options.audio {
            Some(audio) => Some(Self::init_audio_track(
                &ctx,
                &output_id,
                audio,
                &mut output_ctx,
                encoded_chunks_sender.clone(),
            )?),
            None => None,
        };

        // write header initializes time_base
        output_ctx
            .write_header()
            .map_err(OutputInitError::FfmpegError)?;

        let (video_encoder, video_stream) = match video {
            Some((encoder, index)) => (
                Some(encoder),
                Some(Stream {
                    index,
                    time_base: output_ctx.stream(index).unwrap().time_base(),
                }),
            ),
            None => (None, None),
        };

        let (audio_encoder, audio_stream) = match audio {
            Some((encoder, index)) => (
                Some(encoder),
                Some(Stream {
                    index,
                    time_base: output_ctx.stream(index).unwrap().time_base(),
                }),
            ),
            None => (None, None),
        };

        let output_id = output_id.clone();
        let ctx = ctx.clone();
        std::thread::Builder::new()
            .name(format!("RTMP sender thread for output {output_id}"))
            .spawn(move || {
                let _span =
                    tracing::info_span!("RTMP sender  writer", output_id = output_id.to_string())
                        .entered();

                run_ffmpeg_output_thread(
                    output_ctx,
                    video_stream,
                    audio_stream,
                    encoded_chunks_receiver,
                );
                ctx.event_emitter.emit(Event::OutputDone(output_id));
                debug!("Closing RTMP sender thread.");
            })
            .unwrap();

        Ok(Self {
            video: video_encoder,
            audio: audio_encoder,
        })
    }

    fn init_video_track(
        ctx: &Arc<PipelineCtx>,
        output_id: &OutputId,
        options: VideoEncoderOptions,
        output_ctx: &mut ffmpeg::format::context::Output,
        encoded_chunks_sender: Sender<EncodedOutputEvent>,
    ) -> Result<(VideoEncoderThreadHandle, usize), OutputInitError> {
        let resolution = options.resolution();

        let encoder = match &options {
            VideoEncoderOptions::FfmpegH264(options) => {
                spawn_video_encoder_thread::<FfmpegH264Encoder>(
                    ctx.clone(),
                    output_id.clone(),
                    options.clone(),
                    encoded_chunks_sender,
                )?
            }
            VideoEncoderOptions::FfmpegVp8(_) => {
                return Err(OutputInitError::UnsupportedVideoCodec(VideoCodec::Vp8))
            }
            VideoEncoderOptions::FfmpegVp9(_) => {
                return Err(OutputInitError::UnsupportedVideoCodec(VideoCodec::Vp9))
            }
        };

        let mut stream = output_ctx
            .add_stream(ffmpeg::codec::Id::H264)
            .map_err(OutputInitError::FfmpegError)?;

        let codecpar = unsafe { &mut *(*stream.as_mut_ptr()).codecpar };

        if let Some(extradata) = encoder.encoder_context() {
            unsafe {
                // The allocated size of extradata must be at least extradata_size + AV_INPUT_BUFFER_PADDING_SIZE, with the padding bytes zeroed.
                codecpar.extradata = ffmpeg_next::ffi::av_mallocz(
                    extradata.len() + ffmpeg_next::ffi::AV_INPUT_BUFFER_PADDING_SIZE as usize,
                ) as *mut u8;
                std::ptr::copy(extradata.as_ptr(), codecpar.extradata, extradata.len());
                codecpar.extradata_size = extradata.len() as i32;
            };
        }

        codecpar.codec_id = ffmpeg::codec::Id::H264.into();
        codecpar.codec_type = ffmpeg::ffi::AVMediaType::AVMEDIA_TYPE_VIDEO;
        codecpar.width = resolution.width as i32;
        codecpar.height = resolution.height as i32;

        Ok((encoder, stream.index()))
    }

    fn init_audio_track(
        ctx: &Arc<PipelineCtx>,
        output_id: &OutputId,
        options: AudioEncoderOptions,
        output_ctx: &mut ffmpeg::format::context::Output,
        encoded_chunks_sender: Sender<EncodedOutputEvent>,
    ) -> Result<(AudioEncoderThreadHandle, usize), OutputInitError> {
        let channel_count = match options.channels() {
            AudioChannels::Mono => 1,
            AudioChannels::Stereo => 2,
        };
        let sample_rate = options.sample_rate();

        let encoder = match options {
            AudioEncoderOptions::FdkAac(options) => spawn_audio_encoder_thread::<FdkAacEncoder>(
                ctx.clone(),
                output_id.clone(),
                options,
                encoded_chunks_sender,
            )?,
            AudioEncoderOptions::Opus(_) => {
                return Err(OutputInitError::UnsupportedAudioCodec(AudioCodec::Opus))
            }
        };

        let mut stream = output_ctx
            .add_stream(ffmpeg::codec::Id::AAC)
            .map_err(OutputInitError::FfmpegError)?;

        let codecpar = unsafe { &mut *(*stream.as_mut_ptr()).codecpar };
        if let Some(extradata) = encoder.encoder_context() {
            unsafe {
                // The allocated size of extradata must be at least extradata_size + AV_INPUT_BUFFER_PADDING_SIZE, with the padding bytes zeroed.
                codecpar.extradata = ffmpeg_next::ffi::av_mallocz(
                    extradata.len() + ffmpeg_next::ffi::AV_INPUT_BUFFER_PADDING_SIZE as usize,
                ) as *mut u8;
                std::ptr::copy(extradata.as_ptr(), codecpar.extradata, extradata.len());
                codecpar.extradata_size = extradata.len() as i32;
            };
        }
        codecpar.codec_id = ffmpeg::codec::Id::AAC.into();
        codecpar.codec_type = ffmpeg::ffi::AVMediaType::AVMEDIA_TYPE_AUDIO;
        codecpar.sample_rate = sample_rate as i32;
        codecpar.ch_layout = ffmpeg::ffi::AVChannelLayout {
            nb_channels: channel_count,
            order: ffmpeg::ffi::AVChannelOrder::AV_CHANNEL_ORDER_UNSPEC,
            // This value is ignored when order is AV_CHANNEL_ORDER_UNSPEC
            u: ffmpeg::ffi::AVChannelLayout__bindgen_ty_1 { mask: 0 },
            // Field doc: "For some private data of the user."
            opaque: ptr::null_mut(),
        };

        Ok((encoder, stream.index()))
    }
}

impl Output for RtmpClientOutput {
    fn audio(&self) -> Option<OutputAudio> {
        self.audio.as_ref().map(|audio| OutputAudio {
            samples_batch_sender: &audio.sample_batch_sender,
        })
    }

    fn video(&self) -> Option<OutputVideo> {
        self.video.as_ref().map(|video| OutputVideo {
            resolution: video.config.resolution,
            frame_format: video.config.output_format,
            frame_sender: &video.frame_sender,
            keyframe_request_sender: &video.keyframe_request_sender,
        })
    }

    fn kind(&self) -> OutputProtocolKind {
        OutputProtocolKind::Rtmp
    }
}

fn run_ffmpeg_output_thread(
    mut output_ctx: ffmpeg::format::context::Output,
    video_stream: Option<Stream>,
    audio_stream: Option<Stream>,
    packets_receiver: Receiver<EncodedOutputEvent>,
) {
    let mut received_video_eos = video_stream.as_ref().map(|_| false);
    let mut received_audio_eos = audio_stream.as_ref().map(|_| false);

    for packet in packets_receiver {
        match packet {
            EncodedOutputEvent::Data(chunk) => {
                write_chunk(chunk, &video_stream, &audio_stream, &mut output_ctx);
            }
            EncodedOutputEvent::VideoEOS => match received_video_eos {
                Some(false) => received_video_eos = Some(true),
                Some(true) => {
                    error!("Received multiple video EOS events.");
                }
                None => {
                    error!("Received video EOS event on non video output.");
                }
            },
            EncodedOutputEvent::AudioEOS => match received_audio_eos {
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
    chunk: EncodedOutputChunk,
    video_stream: &Option<Stream>,
    audio_stream: &Option<Stream>,
    output_ctx: &mut ffmpeg::format::context::Output,
) {
    let (stream_id, time_base) = match chunk.kind {
        MediaKind::Video(_) => {
            match video_stream {
                Some(Stream { index, time_base }) => (*index, *time_base),
                None => {
                    error!("Failed to create packet for video chunk. No video stream registered on init.");
                    return;
                }
            }
        }
        MediaKind::Audio(_) => {
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

    if chunk.is_keyframe {
        packet.set_flags(ffmpeg_next::packet::Flags::KEY);
    }

    if let Err(err) = packet.write_interleaved(output_ctx) {
        error!("Failed to write packet to RTMP stream: {}.", err);
    }
}
