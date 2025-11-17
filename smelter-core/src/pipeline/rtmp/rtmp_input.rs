use std::{
    ffi::CString,
    ptr, slice,
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};

use bytes::Bytes;
use crossbeam_channel::{Receiver, bounded};
use ffmpeg_next::{
    Dictionary, Stream,
    ffi::{
        EAGAIN, avformat_alloc_context, avformat_close_input, avformat_find_stream_info,
        avformat_open_input,
    },
    format::context,
    util::interrupt,
};
use smelter_render::InputId;
use tracing::{Level, debug, error, span, trace, warn};

use crate::{
    pipeline::{
        decoder::{
            DecoderThreadHandle,
            decoder_thread_audio::{AudioDecoderThread, AudioDecoderThreadOptions},
            decoder_thread_video::{VideoDecoderThread, VideoDecoderThreadOptions},
            fdk_aac, ffmpeg_h264,
            h264_utils::{AvccToAnnexBRepacker, H264AvcDecoderConfig},
            vulkan_h264,
        },
        input::Input,
        rtmp::rtmp_input::{ffmpeg_context::FfmpegInputContext, stream_state::StreamState},
        utils::input_buffer::InputBuffer,
    },
    queue::QueueDataReceiver,
    thread_utils::InitializableThread,
};

use crate::prelude::*;

mod ffmpeg_context;
mod stream_state;

pub struct RtmpServerInput {
    should_close: Arc<AtomicBool>,
}

const RTMP_READ_RETRY_DELAY: Duration = Duration::from_millis(10);

struct Track {
    index: usize,
    handle: DecoderThreadHandle,
    state: StreamState,
}

impl RtmpServerInput {
    pub fn new_input(
        ctx: Arc<PipelineCtx>,
        input_ref: Ref<InputId>,
        opts: RtmpServerInputOptions,
    ) -> Result<(Input, InputInitInfo, QueueDataReceiver), InputInitError> {
        let should_close = Arc::new(AtomicBool::new(false));
        let buffer = InputBuffer::new(&ctx, opts.buffer);

        let (video_sender, frame_receiver) = bounded(5);
        let (audio_sender, samples_receiver) = bounded(5);

        let receivers = QueueDataReceiver {
            video: Some(frame_receiver),
            audio: Some(samples_receiver),
        };

        Self::spawn_connection_thread(
            ctx,
            input_ref.clone(),
            opts,
            should_close.clone(),
            buffer,
            video_sender,
            audio_sender,
        );

        Ok((
            Input::RtmpServer(Self { should_close }),
            InputInitInfo::Other,
            receivers,
        ))
    }

    fn spawn_connection_thread(
        ctx: Arc<PipelineCtx>,
        input_ref: Ref<InputId>,
        opts: RtmpServerInputOptions,
        should_close: Arc<AtomicBool>,
        buffer: InputBuffer,
        video_sender: crossbeam_channel::Sender<PipelineEvent<Frame>>,
        audio_sender: crossbeam_channel::Sender<PipelineEvent<InputAudioSamples>>,
    ) {
        std::thread::Builder::new()
            .name(format!("RTMP connection thread for input {input_ref}"))
            .spawn(move || {
                loop {
                    let _span = span!(
                        Level::INFO,
                        "RTMP connection thread",
                        input_id = input_ref.to_string()
                    )
                    .entered();

                    let input_ctx = match FfmpegInputContext::new(&opts.url, should_close.clone()) {
                        Ok(ctx) => ctx,
                        Err(err) => {
                            error!("Failed to open RTMP input: {err:?}");
                            std::thread::sleep(Duration::from_secs(3));
                            continue;
                        }
                    };
                    let (audio, samples_receiver) = match input_ctx.audio_stream() {
                        Some(stream) => match Self::handle_audio_track(
                            &ctx,
                            &input_ref,
                            &stream,
                            buffer.clone(),
                        ) {
                            Ok((track, receiver)) => (Some(track), Some(receiver)),
                            Err(err) => {
                                error!("Failed to initialize audio track: {err:?}");
                                (None, None)
                            }
                        },
                        None => (None, None),
                    };

                    let (video, frame_receiver) = match input_ctx.video_stream() {
                        Some(stream) => match Self::handle_video_track(
                            &ctx,
                            &input_ref,
                            &stream,
                            opts.video_decoders.clone(),
                            buffer.clone(),
                        ) {
                            Ok((track, receiver)) => (Some(track), Some(receiver)),
                            Err(err) => {
                                error!("Failed to initialize video track: {err:?}");
                                (None, None)
                            }
                        },
                        None => (None, None),
                    };

                    if let Some(receiver) = frame_receiver {
                        let sender = video_sender.clone();
                        std::thread::spawn(move || {
                            while let Ok(event) = receiver.recv() {
                                if sender.send(event).is_err() {
                                    break;
                                }
                            }
                        });
                    }

                    // TODO audio does not work after reconnect
                    if let Some(receiver) = samples_receiver {
                        let sender = audio_sender.clone();
                        std::thread::spawn(move || {
                            while let Ok(event) = receiver.recv() {
                                if sender.send(event).is_err() {
                                    break;
                                }
                            }
                        });
                    }

                    Self::run_demuxer_thread(input_ctx, audio, video);
                    std::thread::sleep(Duration::from_secs(3));
                }
            })
            .unwrap();
    }

    fn handle_audio_track(
        ctx: &Arc<PipelineCtx>,
        input_ref: &Ref<InputId>,
        stream: &Stream<'_>,
        buffer: InputBuffer,
    ) -> Result<(Track, Receiver<PipelineEvent<InputAudioSamples>>), InputInitError> {
        // not tested it was always null, but audio is in ADTS, so config is not
        // necessary
        let asc = read_extra_data(stream);
        let (samples_sender, samples_receiver) = bounded(5);
        let state = StreamState::new(ctx.queue_sync_point, stream.time_base(), buffer);
        let handle = AudioDecoderThread::<fdk_aac::FdkAacDecoder>::spawn(
            input_ref.clone(),
            AudioDecoderThreadOptions {
                ctx: ctx.clone(),
                decoder_options: FdkAacDecoderOptions { asc },
                samples_sender,
                input_buffer_size: 2000,
            },
        )?;

        Ok((
            Track {
                index: stream.index(),
                handle,
                state,
            },
            samples_receiver,
        ))
    }

    fn handle_video_track(
        ctx: &Arc<PipelineCtx>,
        input_ref: &Ref<InputId>,
        stream: &Stream<'_>,
        video_decoders: RtmpServerInputVideoDecoders,
        buffer: InputBuffer,
    ) -> Result<(Track, Receiver<PipelineEvent<Frame>>), InputInitError> {
        let (frame_sender, frame_receiver) = bounded(5);
        let state = StreamState::new(ctx.queue_sync_point, stream.time_base(), buffer);

        let extra_data = read_extra_data(stream);
        let h264_config = extra_data
            .map(H264AvcDecoderConfig::parse)
            .transpose()
            .unwrap_or_else(|e| match e {
                H264AvcDecoderConfigError::NotAVCC => None,
                _ => {
                    warn!("Could not parse extra data: {e}");
                    None
                }
            });

        let decoder_thread_options = VideoDecoderThreadOptions {
            ctx: ctx.clone(),
            transformer: h264_config.map(AvccToAnnexBRepacker::new),
            frame_sender,
            input_buffer_size: 2000,
        };

        let vulkan_supported = ctx.graphics_context.has_vulkan_decoder_support();
        let h264_decoder = video_decoders.h264.unwrap_or({
            match vulkan_supported {
                true => VideoDecoderOptions::VulkanH264,
                false => VideoDecoderOptions::FfmpegH264,
            }
        });

        let handle = match h264_decoder {
            VideoDecoderOptions::FfmpegH264 => {
                VideoDecoderThread::<ffmpeg_h264::FfmpegH264Decoder, _>::spawn(
                    input_ref,
                    decoder_thread_options,
                )?
            }
            VideoDecoderOptions::VulkanH264 => {
                if !vulkan_supported {
                    return Err(InputInitError::DecoderError(
                        DecoderInitError::VulkanContextRequiredForVulkanDecoder,
                    ));
                }
                VideoDecoderThread::<vulkan_h264::VulkanH264Decoder, _>::spawn(
                    input_ref,
                    decoder_thread_options,
                )?
            }
            _ => {
                return Err(InputInitError::InvalidVideoDecoderProvided {
                    expected: VideoCodec::H264,
                });
            }
        };

        Ok((
            Track {
                index: stream.index(),
                handle,
                state,
            },
            frame_receiver,
        ))
    }

    fn run_demuxer_thread(
        mut input_ctx: FfmpegInputContext,
        mut audio: Option<Track>,
        mut video: Option<Track>,
    ) {
        loop {
            let packet = match input_ctx.read_packet() {
                Ok(packet) => packet,
                Err(ffmpeg_next::Error::Eof | ffmpeg_next::Error::Exit) => break,
                Err(ffmpeg_next::Error::Other { errno }) if errno == EAGAIN => {
                    trace!("RTMP demuxer waiting for packets");
                    std::thread::sleep(RTMP_READ_RETRY_DELAY);
                    continue;
                }
                Err(ffmpeg_next::Error::Other { errno: 5 }) => {
                    warn!("Input session disconnected!");
                    break;
                }
                Err(err) => {
                    trace!("RTMP read error {err:?}");
                    continue;
                }
            };

            if packet.is_corrupt() {
                error!(
                    "Corrupted packet {:?} {:?}",
                    packet.stream(),
                    packet.flags()
                );
                continue;
            }

            if let Some(track) = &mut video
                && packet.stream() == track.index
            {
                let (pts, dts) = track.state.pts_dts_from_packet(&packet);

                let chunk = EncodedInputChunk {
                    data: Bytes::copy_from_slice(packet.data().unwrap()),
                    pts,
                    dts,
                    kind: MediaKind::Video(VideoCodec::H264),
                };

                let sender = &track.handle.chunk_sender;
                trace!(?chunk, buffer = sender.len(), "Sending video chunk");
                if sender.is_empty() {
                    debug!("RTMP input video channel was drained");
                }
                if sender.send(PipelineEvent::Data(chunk)).is_err() {
                    debug!("Channel closed")
                }
            }

            if let Some(track) = &mut audio
                && packet.stream() == track.index
            {
                let (pts, dts) = track.state.pts_dts_from_packet(&packet);

                let chunk = EncodedInputChunk {
                    data: bytes::Bytes::copy_from_slice(packet.data().unwrap()),
                    pts,
                    dts,
                    kind: MediaKind::Audio(AudioCodec::Aac),
                };

                let sender = &track.handle.chunk_sender;
                trace!(?chunk, buffer = sender.len(), "Sending audio chunk");
                if sender.is_empty() {
                    debug!("RTMP input audio channel was drained");
                }
                if sender.send(PipelineEvent::Data(chunk)).is_err() {
                    debug!("Channel closed")
                }
            }
        }

        if let Some(Track { handle, .. }) = &audio
            && handle.chunk_sender.send(PipelineEvent::EOS).is_err()
        {
            debug!("Channel closed. Failed to send audio EOS.")
        }

        if let Some(Track { handle, .. }) = &video
            && handle.chunk_sender.send(PipelineEvent::EOS).is_err()
        {
            debug!("Channel closed. Failed to send video EOS.")
        }
    }
}

impl Drop for RtmpServerInput {
    fn drop(&mut self) {
        self.should_close
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

fn read_extra_data(stream: &Stream<'_>) -> Option<Bytes> {
    unsafe {
        let codecpar = (*stream.as_ptr()).codecpar;
        let size = (*codecpar).extradata_size;
        if size > 0 {
            Some(Bytes::copy_from_slice(slice::from_raw_parts(
                (*codecpar).extradata,
                size as usize,
            )))
        } else {
            None
        }
    }
}

/// Combined implementation of ffmpeg_next::format:input_with_interrupt and
/// ffmpeg_next::format::input_with_dictionary that allows passing both interrupt
/// callback and Dictionary with options
fn input_with_dictionary_and_interrupt<F>(
    path: &str,
    options: Dictionary,
    interrupt_fn: F,
) -> Result<context::Input, ffmpeg_next::Error>
where
    F: FnMut() -> bool + 'static,
{
    unsafe {
        let mut ps = avformat_alloc_context();

        (*ps).interrupt_callback = interrupt::new(Box::new(interrupt_fn)).interrupt;

        let path = CString::new(path).unwrap();
        let mut opts = options.disown();
        let res = avformat_open_input(&mut ps, path.as_ptr(), ptr::null_mut(), &mut opts);

        Dictionary::own(opts);

        match res {
            0 => match avformat_find_stream_info(ps, ptr::null_mut()) {
                r if r >= 0 => Ok(context::Input::wrap(ps)),
                e => {
                    avformat_close_input(&mut ps);
                    Err(ffmpeg_next::Error::from(e))
                }
            },

            e => Err(ffmpeg_next::Error::from(e)),
        }
    }
}
