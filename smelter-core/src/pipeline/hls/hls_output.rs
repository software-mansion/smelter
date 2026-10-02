use std::{ptr, sync::Arc};

use crossbeam_channel::{Receiver, Sender, bounded};
use ffmpeg_next::{self as ffmpeg, Rational, Rescale};
use smelter_render::OutputId;
use tracing::{debug, error};

use crate::{
    event::Event,
    pipeline::{
        encoder::{
            encoder_thread_audio::{
                AudioEncoderThread, AudioEncoderThreadHandle, AudioEncoderThreadOptions,
            },
            encoder_thread_video::{
                VideoEncoderThread, VideoEncoderThreadHandle, VideoEncoderThreadOptions,
            },
            fdk_aac::FdkAacEncoder,
            ffmpeg_h264::FfmpegH264Encoder,
            vulkan_h264::VulkanH264Encoder,
        },
        ffmpeg_utils::{
            FfmpegOptions, StreamMutExt, TimestampOffset, warn_unused_options, write_extradata,
        },
        output::{Output, OutputAudio, OutputVideo},
        utils::InitializableThread,
    },
};

use crate::prelude::*;

#[derive(Debug, Clone)]
struct StreamState {
    index: usize,
    time_base: Rational,
    packet_duration: Rational,
}

pub struct HlsOutput {
    video: Option<VideoEncoderThreadHandle>,
    audio: Option<AudioEncoderThreadHandle>,
}

impl HlsOutput {
    pub fn new(
        ctx: Arc<PipelineCtx>,
        output_ref: Ref<OutputId>,
        options: HlsOutputOptions,
    ) -> Result<Self, OutputInitError> {
        let start_at = options.start_at;
        let (encoded_chunks_sender, encoded_chunks_receiver) = bounded(1);

        ctx.stats_sender.send(StatsEvent::NewOutput {
            output_ref: output_ref.clone(),
            kind: OutputProtocolKind::Hls,
        });

        let mut output_ctx = ffmpeg::format::output_as(&options.output_path, "hls")
            .map_err(OutputInitError::FfmpegError)?;

        let video = match options.video {
            Some(video) => Some(Self::init_video_track(
                &ctx,
                &output_ref,
                video,
                &mut output_ctx,
                encoded_chunks_sender.clone(),
            )?),
            None => None,
        };
        let audio = match options.audio {
            Some(audio) => Some(Self::init_audio_track(
                &ctx,
                &output_ref,
                audio,
                &mut output_ctx,
                encoded_chunks_sender.clone(),
            )?),
            None => None,
        };

        let mut ffmpeg_options = FfmpegOptions::from(&[
            ("hls_flags", "delete_segments"),
            (
                "hls_list_size",
                // 0 means no list size limit
                &options.max_playlist_size.unwrap_or(0).to_string(),
            ),
        ]);
        ffmpeg_options.append(&options.raw_options);

        warn_unused_options(
            &output_ctx
                .write_header_with(ffmpeg_options.into_dictionary())
                .map_err(OutputInitError::FfmpegError)?,
            "HLS muxer",
        );

        let (video_encoder, video_stream) = match video {
            Some((encoder, index, packet_duration)) => (
                Some(encoder),
                Some(StreamState {
                    index,
                    time_base: output_ctx.stream(index).unwrap().time_base(),
                    packet_duration,
                }),
            ),
            None => (None, None),
        };

        let (audio_encoder, audio_stream) = match audio {
            Some((encoder, index, packet_duration)) => (
                Some(encoder),
                Some(StreamState {
                    index,
                    time_base: output_ctx.stream(index).unwrap().time_base(),
                    packet_duration,
                }),
            ),
            None => (None, None),
        };

        let offset = TimestampOffset::new(
            ctx.queue_ctx.clone(),
            start_at,
            video_stream.is_some(),
            audio_stream.is_some(),
            None,
        );

        std::thread::Builder::new()
            .name(format!("HLS writer thread for output {output_ref}"))
            .spawn(move || {
                let _span =
                    tracing::info_span!("HLS writer", output_id = output_ref.to_string()).entered();

                let stats_sender = HlsOutputStatsSender {
                    stats_sender: ctx.stats_sender.clone(),
                    output_ref: output_ref.clone(),
                };
                run_ffmpeg_output_thread(
                    &ctx,
                    &output_ref,
                    output_ctx,
                    video_stream,
                    audio_stream,
                    encoded_chunks_receiver,
                    stats_sender,
                    offset,
                );
                ctx.event_emitter
                    .emit(Event::OutputDone(output_ref.id().clone()));
                debug!("Closing HLS writer thread.");
            })
            .unwrap();

        Ok(HlsOutput {
            video: video_encoder,
            audio: audio_encoder,
        })
    }

    fn init_video_track(
        ctx: &Arc<PipelineCtx>,
        output_id: &Ref<OutputId>,
        options: VideoEncoderOptions,
        output_ctx: &mut ffmpeg::format::context::Output,
        encoded_chunks_sender: Sender<EncodedOutputEvent>,
    ) -> Result<(VideoEncoderThreadHandle, usize, Rational), OutputInitError> {
        let resolution = options.resolution();

        let encoder = match &options {
            VideoEncoderOptions::FfmpegH264(options) => {
                VideoEncoderThread::<FfmpegH264Encoder>::spawn(
                    output_id.clone(),
                    VideoEncoderThreadOptions {
                        ctx: ctx.clone(),
                        encoder_options: options.clone(),
                        chunks_sender: encoded_chunks_sender,
                    },
                )?
            }
            VideoEncoderOptions::VulkanH264(options) => {
                if !ctx.graphics_context.has_vulkan_encoder_support() {
                    return Err(OutputInitError::EncoderError(
                        EncoderInitError::VulkanContextRequiredForVulkanEncoder,
                    ));
                }
                VideoEncoderThread::<VulkanH264Encoder>::spawn(
                    output_id.clone(),
                    VideoEncoderThreadOptions {
                        ctx: ctx.clone(),
                        encoder_options: options.clone(),
                        chunks_sender: encoded_chunks_sender,
                    },
                )?
            }
            VideoEncoderOptions::FfmpegVp8(_) => {
                return Err(OutputInitError::UnsupportedVideoCodec(VideoCodec::Vp8));
            }
            VideoEncoderOptions::FfmpegVp9(_) => {
                return Err(OutputInitError::UnsupportedVideoCodec(VideoCodec::Vp9));
            }
        };

        let mut stream = output_ctx
            .add_stream(ffmpeg::codec::Id::H264)
            .map_err(OutputInitError::FfmpegError)?;

        stream.set_time_base(VIDEO_TIME_BASE);
        stream.update_codecpar(|codecpar| {
            if let Some(extradata) = encoder.encoder_context() {
                write_extradata(codecpar, extradata);
            }

            codecpar.codec_id = ffmpeg::codec::Id::H264.into();
            codecpar.codec_type = ffmpeg::ffi::AVMediaType::AVMEDIA_TYPE_VIDEO;
            codecpar.width = resolution.width as i32;
            codecpar.height = resolution.height as i32;
        });

        let framerate = ctx.output_framerate;
        let packet_duration = Rational(framerate.den as i32, framerate.num as i32);
        Ok((encoder, stream.index(), packet_duration))
    }

    fn init_audio_track(
        ctx: &Arc<PipelineCtx>,
        output_id: &Ref<OutputId>,
        options: AudioEncoderOptions,
        output_ctx: &mut ffmpeg::format::context::Output,
        encoded_chunks_sender: Sender<EncodedOutputEvent>,
    ) -> Result<(AudioEncoderThreadHandle, usize, Rational), OutputInitError> {
        let channel_count = match options.channels() {
            AudioChannels::Mono => 1,
            AudioChannels::Stereo => 2,
        };
        let sample_rate = options.sample_rate();

        let encoder = match options {
            AudioEncoderOptions::FdkAac(options) => AudioEncoderThread::<FdkAacEncoder>::spawn(
                output_id.clone(),
                AudioEncoderThreadOptions {
                    ctx: ctx.clone(),
                    encoder_options: options,
                    chunks_sender: encoded_chunks_sender,
                },
            )?,
            AudioEncoderOptions::Opus(_) => {
                return Err(OutputInitError::UnsupportedAudioCodec(AudioCodec::Opus));
            }
        };

        let mut stream = output_ctx
            .add_stream(ffmpeg::codec::Id::AAC)
            .map_err(OutputInitError::FfmpegError)?;

        stream.update_codecpar(|codecpar| {
            if let Some(extradata) = encoder.encoder_context() {
                write_extradata(codecpar, extradata);
            }
            codecpar.codec_id = ffmpeg::codec::Id::AAC.into();
            codecpar.codec_type = ffmpeg::ffi::AVMediaType::AVMEDIA_TYPE_AUDIO;
            codecpar.sample_rate = sample_rate as i32;
            codecpar.profile = ffmpeg::ffi::FF_PROFILE_AAC_LOW;
            codecpar.ch_layout = ffmpeg::ffi::AVChannelLayout {
                nb_channels: channel_count,
                order: ffmpeg::ffi::AVChannelOrder::AV_CHANNEL_ORDER_UNSPEC,
                // This value is ignored when order is AV_CHANNEL_ORDER_UNSPEC
                u: ffmpeg::ffi::AVChannelLayout__bindgen_ty_1 { mask: 0 },
                // Field doc: "For some private data of the user."
                opaque: ptr::null_mut(),
            };
        });

        let packet_duration = Rational(encoder.config.samples_per_frame as i32, sample_rate as i32);
        Ok((encoder, stream.index(), packet_duration))
    }
}

impl Output for HlsOutput {
    fn audio(&self) -> Option<OutputAudio<'_>> {
        self.audio.as_ref().map(|audio| OutputAudio {
            samples_batch_sender: &audio.sample_batch_sender,
        })
    }

    fn video(&self) -> Option<OutputVideo<'_>> {
        self.video.as_ref().map(|video| OutputVideo {
            resolution: video.config.resolution,
            frame_format: video.config.output_format,
            frame_sender: &video.frame_sender,
            keyframe_request_sender: &video.keyframe_request_sender,
        })
    }

    fn kind(&self) -> OutputProtocolKind {
        OutputProtocolKind::Hls
    }
}

const VIDEO_TIME_BASE: Rational = Rational(1, 90_000);
const NS_TIME_BASE: Rational = Rational(1, 1_000_000_000);

#[allow(clippy::too_many_arguments)]
fn run_ffmpeg_output_thread(
    ctx: &Arc<PipelineCtx>,
    output_ref: &Ref<OutputId>,
    mut output_ctx: ffmpeg::format::context::Output,
    mut video_stream: Option<StreamState>,
    mut audio_stream: Option<StreamState>,
    packets_receiver: Receiver<EncodedOutputEvent>,
    stats_sender: HlsOutputStatsSender,
    mut offset: TimestampOffset,
) {
    let mut received_video_eos = video_stream.as_ref().map(|_| false);
    let mut received_audio_eos = audio_stream.as_ref().map(|_| false);

    'packets: for packet in packets_receiver.into_iter().map(Some).chain([None]) {
        let chunks = match packet {
            Some(EncodedOutputEvent::Data(chunk)) => offset.resolve(chunk),
            Some(EncodedOutputEvent::VideoEOS) => {
                match received_video_eos {
                    Some(false) => received_video_eos = Some(true),
                    Some(true) => {
                        error!("Received multiple video EOS events.");
                    }
                    None => {
                        error!("Received video EOS event on non video output.");
                    }
                }
                offset.on_track_eos(MediaKind::Video(VideoCodec::H264))
            }
            Some(EncodedOutputEvent::AudioEOS) => {
                match received_audio_eos {
                    Some(false) => received_audio_eos = Some(true),
                    Some(true) => {
                        error!("Received multiple audio EOS events.");
                    }
                    None => {
                        error!("Received audio EOS event on non audio output.");
                    }
                }
                offset.on_track_eos(MediaKind::Audio(AudioCodec::Aac))
            }
            None => offset.flush(),
        };

        for (timestamp_offset, chunk) in chunks {
            stats_sender.bytes_sent_event(chunk.data.len(), chunk.kind.into());
            let result = write_chunk(
                chunk,
                &mut video_stream,
                &mut audio_stream,
                &mut output_ctx,
                timestamp_offset,
            );
            if let Err(err) = result {
                let try_write_trailer = !matches!(err, OutputHlsRuntimeError::NoSpaceLeftOnDevice);
                ctx.event_emitter.emit(Event::OutputError {
                    output_id: output_ref.id().clone(),
                    err: err.into(),
                    severity: ErrorSeverity::Critical,
                });
                match try_write_trailer {
                    true => break 'packets,
                    false => return,
                }
            }
        }

        if received_video_eos.unwrap_or(true) && received_audio_eos.unwrap_or(true) {
            break;
        }
    }

    if let Err(err) = output_ctx.write_trailer() {
        let err = match err {
            ffmpeg::Error::Other {
                errno: ffmpeg::error::ENOSPC,
            } => OutputHlsRuntimeError::NoSpaceLeftOnDevice,
            err => OutputHlsRuntimeError::TrailerWriteError(err),
        };
        ctx.event_emitter.emit(Event::OutputError {
            output_id: output_ref.id().clone(),
            err: err.into(),
            severity: ErrorSeverity::Critical,
        });
    };
}

fn write_chunk(
    chunk: EncodedOutputChunk,
    video_stream: &mut Option<StreamState>,
    audio_stream: &mut Option<StreamState>,
    output_ctx: &mut ffmpeg::format::context::Output,
    timestamp_offset: Timestamp,
) -> Result<(), OutputHlsRuntimeError> {
    let stream = match chunk.kind {
        MediaKind::Video(_) => match video_stream {
            Some(stream) => stream,
            None => {
                error!(
                    "Failed to create packet for video chunk. No video stream registered on init."
                );
                return Ok(());
            }
        },
        MediaKind::Audio(_) => match audio_stream {
            Some(stream) => stream,
            None => {
                error!(
                    "Failed to create packet for audio chunk. No audio stream registered on init."
                );
                return Ok(());
            }
        },
    };

    let pts = chunk.pts - timestamp_offset;
    let dts = chunk.dts.map_or(pts, |dts| dts - timestamp_offset);

    let mut packet = ffmpeg::Packet::copy(&chunk.data);
    packet.set_pts(Some(Rescale::rescale(
        &pts.as_nanos(),
        NS_TIME_BASE,
        stream.time_base,
    )));
    packet.set_dts(Some(Rescale::rescale(
        &dts.as_nanos(),
        NS_TIME_BASE,
        stream.time_base,
    )));
    // Convert from timeline when single packet is a single tick, to stream time
    // base timeline. Returns duration of single packet in time_base units.
    packet.set_duration(Rescale::rescale(
        &1,
        stream.packet_duration,
        stream.time_base,
    ));
    packet.set_time_base(stream.time_base);
    packet.set_stream(stream.index);

    if chunk.is_keyframe {
        packet.set_flags(ffmpeg::packet::Flags::KEY)
    }

    // Encoders deliver packets in their own output order (video lags behind audio by the
    // encoder delay). Interleaving by dts keeps audio in the segment of its matching video.
    packet
        .write_interleaved(output_ctx)
        .map_err(|err| match err {
            ffmpeg::Error::Other {
                errno: ffmpeg::error::ENOSPC,
            } => OutputHlsRuntimeError::NoSpaceLeftOnDevice,
            err => OutputHlsRuntimeError::PacketWriteError(err),
        })?;
    Ok(())
}

struct HlsOutputStatsSender {
    stats_sender: StatsSender,
    output_ref: Ref<OutputId>,
}

impl HlsOutputStatsSender {
    fn bytes_sent_event(&self, size: usize, track_kind: StatsTrackKind) {
        self.stats_sender.send(
            HlsOutputTrackStatsEvent::BytesSent(size).into_event(&self.output_ref, track_kind),
        );
    }
}
