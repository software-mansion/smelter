use std::{
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};

use crossbeam_channel::Sender;
use smelter_render::InputId;
use tracing::{Level, debug, error, span, warn};

use crate::pipeline::{
    rtmp::rtmp_input::{
        Track, demux::run_demuxer_thread, ffmpeg_context::FfmpegInputContext,
        track_audio::handle_audio_track, track_video::handle_video_track,
    },
    utils::input_buffer::InputBuffer,
};

use crate::prelude::*;

pub(super) fn spawn_initialization_thread(
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

            let mut audio_track: Option<Track> = None;
            let mut video_track: Option<Track> = None;

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

                if audio_track.is_none()
                    && let Some(stream) = input_ctx.audio_stream()
                {
                    match handle_audio_track(
                        &ctx,
                        &input_ref,
                        &stream,
                        buffer.clone(),
                        samples_sender.clone(),
                    ) {
                        Ok(track) => {
                            audio_track = Some(track);
                        }
                        Err(err) => {
                            error!("Failed to initialize audio track: {err:?}");
                        }
                    }
                }

                if video_track.is_none()
                    && let Some(stream) = input_ctx.video_stream()
                {
                    match handle_video_track(
                        &ctx,
                        &input_ref,
                        &stream,
                        opts.video_decoders.clone(),
                        buffer.clone(),
                        frame_sender.clone(),
                    ) {
                        Ok(track) => {
                            video_track = Some(track);
                        }
                        Err(err) => {
                            error!("Failed to initialize video track: {err:?}");
                        }
                    }
                }

                run_demuxer_thread(input_ctx, audio_track.as_mut(), video_track.as_mut());

                warn!("RTMP connection lost, reconnecting possible in 3s...");
                std::thread::sleep(Duration::from_secs(3));
            }

            if let Some(Track { handle, .. }) = &audio_track
                && handle.chunk_sender.send(PipelineEvent::EOS).is_err()
            {
                debug!("Channel closed. Failed to send audio EOS.")
            }

            if let Some(Track { handle, .. }) = &video_track
                && handle.chunk_sender.send(PipelineEvent::EOS).is_err()
            {
                debug!("Channel closed. Failed to send video EOS.")
            }
        })
        .unwrap();
}
