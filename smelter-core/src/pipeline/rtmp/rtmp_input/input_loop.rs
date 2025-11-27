use std::{
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};

use crossbeam_channel::{Receiver, Sender, bounded};
use smelter_render::InputId;
use tracing::{Level, debug, error, span, warn};

use crate::{
    pipeline::{
        decoder::{
            decoder_thread_audio::{AudioDecoderThread, AudioDecoderThreadOptions},
            decoder_thread_video::{VideoDecoderThread, VideoDecoderThreadOptions},
            fdk_aac, ffmpeg_h264, vulkan_h264,
        },
        rtmp::rtmp_input::{
            StreamState, Track, demux::run_demuxer_loop, ffmpeg_context::FfmpegInputContext,
            ffmpeg_utils::read_extra_data,
        },
        utils::{H264AvcDecoderConfig, H264AvccToAnnexB, input_buffer::InputBuffer},
    },
    thread_utils::InitializableThread,
};

use crate::prelude::*;

pub(super) fn spawn_input_loop(
    ctx: Arc<PipelineCtx>,
    input_ref: Ref<InputId>,
    opts: RtmpServerInputOptions,
    should_close: Arc<AtomicBool>,
    buffer: InputBuffer,
    frame_sender: Sender<PipelineEvent<Frame>>,
    samples_sender: Sender<PipelineEvent<InputAudioSamples>>,
) {
    std::thread::Builder::new()
        .name(format!("RTMP thread for input {input_ref}"))
        .spawn(move || {
            let _span =
                span!(Level::INFO, "RTMP thread", input_id = input_ref.to_string()).entered();

            loop {
                if should_close.load(std::sync::atomic::Ordering::Relaxed) {
                    break;
                }

                let input_ctx = match FfmpegInputContext::new(&opts.url, should_close.clone()) {
                    Ok(ctx) => ctx,
                    Err(err) => {
                        error!("Failed to open RTMP input: {err:?}");
                        std::thread::sleep(Duration::from_secs(3));
                        continue;
                    }
                };

                let audio_track =
                    setup_audio_track(&ctx, &input_ctx, &input_ref, &buffer, &samples_sender);

                let video_track =
                    setup_video_track(&ctx, &input_ctx, &input_ref, &opts, &buffer, &frame_sender);

                run_demuxer_loop(input_ctx, audio_track, video_track);

                warn!("RTMP connection lost, reconnecting possible in 3s...");
                std::thread::sleep(Duration::from_secs(3));
            }

            if frame_sender.send(PipelineEvent::EOS).is_err() {
                debug!("Channel closed. Failed to send video EOS.")
            }

            if samples_sender.send(PipelineEvent::EOS).is_err() {
                debug!("Channel closed. Failed to send audio EOS.")
            }
        })
        .unwrap();
}

fn setup_audio_track(
    ctx: &Arc<PipelineCtx>,
    input_ctx: &FfmpegInputContext,
    input_ref: &Ref<InputId>,
    buffer: &InputBuffer,
    samples_sender: &Sender<PipelineEvent<InputAudioSamples>>,
) -> Option<Track> {
    let stream = input_ctx.audio_stream()?;
    let asc = read_extra_data(&stream);
    let state = StreamState::new(ctx.queue_sync_point, stream.time_base(), buffer.clone());

    let (decoder_sender, decoder_receiver) = bounded(10);
    spawn_forwarder(
        input_ref.clone(),
        decoder_receiver,
        samples_sender.clone(),
        "Audio",
    );

    let handle = AudioDecoderThread::<fdk_aac::FdkAacDecoder>::spawn(
        input_ref.clone(),
        AudioDecoderThreadOptions {
            ctx: ctx.clone(),
            decoder_options: FdkAacDecoderOptions { asc },
            samples_sender: decoder_sender.clone(),
            input_buffer_size: 10,
        },
    );

    match handle {
        Ok(handle) => Some(Track {
            index: stream.index(),
            handle,
            state,
        }),
        Err(err) => {
            error!("Failed to initialize audio track: {err:?}");
            None
        }
    }
}

fn setup_video_track(
    ctx: &Arc<PipelineCtx>,
    input_ctx: &FfmpegInputContext,
    input_ref: &Ref<InputId>,
    opts: &RtmpServerInputOptions,
    buffer: &InputBuffer,
    frame_sender: &Sender<PipelineEvent<Frame>>,
) -> Option<Track> {
    let stream = input_ctx.video_stream()?;
    let state = StreamState::new(ctx.queue_sync_point, stream.time_base(), buffer.clone());

    let extra_data = read_extra_data(&stream);
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

    let (decoder_sender, decoder_receiver) = bounded(10);
    spawn_forwarder(
        input_ref.clone(),
        decoder_receiver,
        frame_sender.clone(),
        "Video",
    );

    let decoder_thread_options = VideoDecoderThreadOptions {
        ctx: ctx.clone(),
        transformer: h264_config.map(H264AvccToAnnexB::new),
        frame_sender: decoder_sender.clone(),
        input_buffer_size: 10,
    };

    let vulkan_supported = ctx.graphics_context.has_vulkan_decoder_support();
    let h264_decoder = opts.video_decoders.h264.unwrap_or({
        match vulkan_supported {
            true => VideoDecoderOptions::VulkanH264,
            false => VideoDecoderOptions::FfmpegH264,
        }
    });

    let handle = match h264_decoder {
        VideoDecoderOptions::FfmpegH264 => {
            VideoDecoderThread::<ffmpeg_h264::FfmpegH264Decoder, _>::spawn(
                input_ref.clone(),
                decoder_thread_options,
            )
        }
        VideoDecoderOptions::VulkanH264 => {
            VideoDecoderThread::<vulkan_h264::VulkanH264Decoder, _>::spawn(
                input_ref.clone(),
                decoder_thread_options,
            )
        }
        _ => {
            error!("Invalid video decoder provided, expected H264");
            return None;
        }
    };

    match handle {
        Ok(handle) => Some(Track {
            index: stream.index(),
            handle,
            state,
        }),
        Err(err) => {
            error!("Failed to initialize video track: {err:?}");
            None
        }
    }
}

fn spawn_forwarder<T: Send + 'static>(
    input_ref: Ref<InputId>,
    receiver: Receiver<PipelineEvent<T>>,
    sender: Sender<PipelineEvent<T>>,
    media_kind: &str,
) {
    std::thread::Builder::new()
        .name(format!("{media_kind} forwarder for input {input_ref}"))
        .spawn(move || {
            for event in receiver {
                if let PipelineEvent::EOS = event {
                    break;
                }
                if sender.send(event).is_err() {
                    break;
                }
            }
        })
        .unwrap();
}
