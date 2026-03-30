use std::{
    fs::File,
    path::Path,
    sync::Arc,
    thread::{self, JoinHandle, ThreadId},
    time::Duration,
};

use crossbeam_channel::{Receiver, Sender, unbounded};
use tracing::{Level, debug, error, span, trace};

use crate::{
    pipeline::{
        decoder::{
            DecoderThreadHandle,
            decoder_thread_audio::{AudioDecoderThread, AudioDecoderThreadOptions},
            decoder_thread_video::{VideoDecoderThread, VideoDecoderThreadOptions},
            fdk_aac, ffmpeg_h264, vulkan_h264,
        },
        input::Input,
        mp4::reader::{DecoderOptions, Mp4FileReader, Track},
        utils::{H264AvccToAnnexB, input_buffer::InputBuffer},
    },
    queue::{AudioTrackOptions, QueueInput, VideoTrackOptions, WeakQueueInput},
    utils::{InitializableThread, ShutdownCondition},
};

use crate::prelude::*;

/// Channel size between input and decoder
const CHUNK_BUFFER_SIZE: usize = 1;

pub struct Mp4Input {
    events_sender: Sender<StateEvent>,
}

impl Mp4Input {
    pub fn seek(&self, position: Duration) {
        if self.events_sender.send(StateEvent::Seek(position)).is_err() {
            debug!("Failed to handle seek event. Channel closed.")
        }
    }

    pub fn pause(&self) {
        if self.events_sender.send(StateEvent::Pause).is_err() {
            debug!("Failed to handle pause event. Channel closed.")
        }
    }

    pub fn resume(&self) {
        if self.events_sender.send(StateEvent::Resume).is_err() {
            debug!("Failed to handle resume event. Channel closed.")
        }
    }
}

impl Mp4Input {
    pub fn new_input(
        ctx: Arc<PipelineCtx>,
        input_ref: Ref<InputId>,
        options: Mp4InputOptions,
    ) -> Result<(Input, InputInitInfo, QueueInput), InputInitError> {
        let source_file = match options.source.clone() {
            Mp4InputSource::Url(url) => Self::download_remote_file(&ctx, &url)?,
            Mp4InputSource::File(path) => Arc::new(SourceFile {
                path,
                remove_on_drop: false,
            }),
        };

        ctx.stats_sender.send(StatsEvent::NewInput {
            input_ref: input_ref.clone(),
            kind: InputProtocolKind::Mp4,
        });

        let video = Mp4FileReader::from_path(&source_file.path)?.try_new_h264_track();
        let video_duration = video.as_ref().and_then(|track| track.duration());
        let audio = Mp4FileReader::from_path(&source_file.path)?.try_new_aac_track();
        let audio_duration = audio.as_ref().and_then(|track| track.duration());

        if video.is_none() && audio.is_none() {
            return Err(Mp4InputError::NoTrack.into());
        }

        let vulkan_supported = ctx.graphics_context.has_vulkan_decoder_support();
        let h264_decoder = options.video_decoders.h264.unwrap_or({
            if vulkan_supported {
                VideoDecoderOptions::VulkanH264
            } else {
                VideoDecoderOptions::FfmpegH264
            }
        });

        let mut queue_input = QueueInput::new(&ctx, &input_ref, options.required);
        let (video_sender, audio_sender) = queue_input.queue_new_track(
            video.as_ref().map(|_| VideoTrackOptions {
                offset: options.offset,
            }),
            audio.as_ref().map(|_| AudioTrackOptions {
                offset: options.offset,
            }),
        );

        let (video_handle, video_track) = match (video, video_sender) {
            (Some(track), Some(frame_sender)) => {
                let handle = match (track.decoder_options(), h264_decoder) {
                    (DecoderOptions::H264(h264_config), VideoDecoderOptions::FfmpegH264) => {
                        VideoDecoderThread::<ffmpeg_h264::FfmpegH264Decoder, _>::spawn(
                            input_ref.clone(),
                            VideoDecoderThreadOptions {
                                ctx: ctx.clone(),
                                transformer: Some(H264AvccToAnnexB::new(h264_config.clone())),
                                frame_sender,
                                input_buffer_size: CHUNK_BUFFER_SIZE,
                            },
                        )?
                    }
                    (DecoderOptions::H264(h264_config), VideoDecoderOptions::VulkanH264) => {
                        if !vulkan_supported {
                            return Err(InputInitError::DecoderError(
                                DecoderInitError::VulkanContextRequiredForVulkanDecoder,
                            ));
                        }
                        VideoDecoderThread::<vulkan_h264::VulkanH264Decoder, _>::spawn(
                            input_ref.clone(),
                            VideoDecoderThreadOptions {
                                ctx: ctx.clone(),
                                transformer: Some(H264AvccToAnnexB::new(h264_config.clone())),
                                frame_sender,
                                input_buffer_size: CHUNK_BUFFER_SIZE,
                            },
                        )?
                    }
                    _ => {
                        return Err(
                            Mp4InputError::Unknown("Non H264 decoder options returned.").into()
                        );
                    }
                };
                (Some(handle), Some(track))
            }
            _ => (None, None),
        };

        let (audio_handle, audio_track) = match (audio, audio_sender) {
            (Some(track), Some(samples_sender)) => {
                let handle = match track.decoder_options() {
                    DecoderOptions::Aac(data) => {
                        AudioDecoderThread::<fdk_aac::FdkAacDecoder>::spawn(
                            input_ref.clone(),
                            AudioDecoderThreadOptions {
                                ctx: ctx.clone(),
                                decoder_options: FdkAacDecoderOptions {
                                    asc: Some(data.clone()),
                                },
                                samples_sender,
                                input_buffer_size: CHUNK_BUFFER_SIZE,
                            },
                        )?
                    }
                    _ => {
                        return Err(
                            Mp4InputError::Unknown("Non AAC decoder options returned.").into()
                        );
                    }
                };
                (Some(handle), Some(track))
            }
            _ => (None, None),
        };

        let (reader, events_sender) = TrackManagerThread::new(
            &ctx,
            &input_ref,
            options,
            source_file,
            video_handle,
            audio_handle,
            queue_input.downgrade(),
        );
        std::thread::Builder::new()
            .name("mp4 reader".to_string())
            .spawn(move || {
                reader.run(video_track, audio_track);
            })
            .unwrap();

        Ok((
            Input::Mp4(Self { events_sender }),
            InputInitInfo::Mp4 {
                video_duration,
                audio_duration,
            },
            queue_input,
        ))
    }

    fn download_remote_file(
        ctx: &Arc<PipelineCtx>,
        url: &str,
    ) -> Result<Arc<SourceFile>, Mp4InputError> {
        let file_response = reqwest::blocking::get(url)?;
        let mut file_response = file_response.error_for_status()?;

        let path = ctx
            .download_dir
            .join(format!("smelter-user-file-{}.mp4", rand::random::<u64>()));

        let mut file = std::fs::File::create(&path)?;

        std::io::copy(&mut file_response, &mut file)?;

        Ok(Arc::new(SourceFile {
            path: path.into(),
            remove_on_drop: true,
        }))
    }
}

impl Drop for Mp4Input {
    fn drop(&mut self) {
        if self.events_sender.send(StateEvent::InputShutdown).is_err() {
            error!("Failed to send InputShutdown event. Channel closed")
        }
    }
}

enum StateEvent {
    Seek(Duration),
    Pause,
    Resume,
    ThreadFinished(ThreadId),
    InputShutdown,
}

#[derive(Clone)]
struct TrackContext {
    input_ref: Ref<InputId>,
    buffer: InputBuffer,

    event_sender: Sender<StateEvent>,
    stats_sender: StatsSender,
    decoder_handle: DecoderThreadHandle,

    _source_file: Arc<SourceFile>,
}

struct TrackManagerThread {
    ctx: Arc<PipelineCtx>,
    input_ref: Ref<InputId>,
    options: Mp4InputOptions,
    events_receiver: Receiver<StateEvent>,
    input_shutdown_condition: ShutdownCondition,
    video_ctx: Option<TrackContext>,
    audio_ctx: Option<TrackContext>,
    video_thread: Option<(JoinHandle<TrackThreadResult>, ShutdownCondition)>,
    audio_thread: Option<(JoinHandle<TrackThreadResult>, ShutdownCondition)>,
    queue_input: WeakQueueInput,
}

impl TrackManagerThread {
    fn new(
        ctx: &Arc<PipelineCtx>,
        input_ref: &Ref<InputId>,
        options: Mp4InputOptions,
        source_file: Arc<SourceFile>,
        video_handle: Option<DecoderThreadHandle>,
        audio_handle: Option<DecoderThreadHandle>,
        queue_input: WeakQueueInput,
    ) -> (Self, Sender<StateEvent>) {
        let (events_sender, events_receiver) = unbounded();
        let buffer = InputBuffer::new(ctx, options.buffer);

        let video_ctx = video_handle.map(|handle| TrackContext {
            input_ref: input_ref.clone(),
            buffer: buffer.clone(),
            event_sender: events_sender.clone(),
            stats_sender: ctx.stats_sender.clone(),
            decoder_handle: handle,
            _source_file: source_file.clone(),
        });

        let audio_ctx = audio_handle.map(|handle| TrackContext {
            input_ref: input_ref.clone(),
            buffer: buffer.clone(),
            event_sender: events_sender.clone(),
            stats_sender: ctx.stats_sender.clone(),
            decoder_handle: handle,
            _source_file: source_file.clone(),
        });

        (
            Self {
                ctx: ctx.clone(),
                input_ref: input_ref.clone(),
                options,
                events_receiver,
                input_shutdown_condition: ShutdownCondition::default(),
                video_ctx,
                audio_ctx,
                video_thread: None,
                audio_thread: None,
                queue_input,
            },
            events_sender,
        )
    }

    fn run(mut self, video_track: Option<Track<File>>, audio_track: Option<Track<File>>) {
        if let (Some(track), Some(ctx)) = (video_track, &self.video_ctx) {
            self.video_thread = Some(self.spawn_video(ctx, track, self.options.seek));
        }
        if let (Some(track), Some(ctx)) = (audio_track, &self.audio_ctx) {
            self.audio_thread = Some(self.spawn_audio(ctx, track, self.options.seek));
        }

        while let Ok(event) = self.events_receiver.recv() {
            match event {
                StateEvent::Pause => {
                    let Some(input) = self.queue_input.upgrade() else {
                        return;
                    };
                    input.pause();
                }
                StateEvent::Resume => {
                    let Some(input) = self.queue_input.upgrade() else {
                        return;
                    };
                    input.resume();
                }
                StateEvent::Seek(seek) => {
                    self.restart_threads(Some(seek));
                }
                StateEvent::ThreadFinished(thread_id) => {
                    match self.options.should_loop {
                        false => {
                            // do not break because user can still
                            // send seek request
                        }
                        true => {
                            if let Some((video, _)) = &self.video_thread
                                && video.thread().id() == thread_id
                            {
                                self.restart_threads(None);
                            }

                            if let Some((audio, _)) = &self.audio_thread
                                && audio.thread().id() == thread_id
                            {
                                self.restart_threads(None);
                            }
                        }
                    }
                }
                StateEvent::InputShutdown => {
                    self.input_shutdown_condition.mark_for_shutdown();
                    return;
                }
            }
        }
    }

    fn restart_threads(&mut self, seek: Option<Duration>) {
        if let Some((_, cond)) = self.video_thread.as_ref() {
            cond.mark_for_shutdown()
        }
        if let Some((_, cond)) = self.audio_thread.as_ref() {
            cond.mark_for_shutdown()
        }

        let video = self
            .video_thread
            .take()
            .map(|(handle, _)| handle.join().unwrap());
        let audio = self
            .audio_thread
            .take()
            .map(|(handle, _)| handle.join().unwrap());

        if let (Some(result), Some(ctx)) = (video, &self.video_ctx) {
            self.video_thread = Some(self.spawn_video(ctx, result.track, seek));
        }
        if let (Some(result), Some(ctx)) = (audio, &self.audio_ctx) {
            self.audio_thread = Some(self.spawn_audio(ctx, result.track, seek));
        }
    }

    fn spawn_video(
        &self,
        ctx: &TrackContext,
        track: Track<File>,
        seek: Option<Duration>,
    ) -> (JoinHandle<TrackThreadResult>, ShutdownCondition) {
        let shutdown_condition = self.input_shutdown_condition.child_condition();
        let track_thread = TrackThread {
            ctx: ctx.clone(),
            shutdown_condition: shutdown_condition.clone(),
            track,
            seek,
        };
        let input_id = self.input_ref.to_string();
        let handle = std::thread::Builder::new()
            .name("mp4 reader - video".to_string())
            .spawn(move || {
                let _span = span!(Level::INFO, "MP4 video", input_id = input_id).entered();
                track_thread.run_video_thread()
            })
            .unwrap();
        (handle, shutdown_condition)
    }

    fn spawn_audio(
        &self,
        ctx: &TrackContext,
        track: Track<File>,
        seek: Option<Duration>,
    ) -> (JoinHandle<TrackThreadResult>, ShutdownCondition) {
        let shutdown_condition = self.input_shutdown_condition.child_condition();
        let track_thread = TrackThread {
            ctx: ctx.clone(),
            shutdown_condition: shutdown_condition.clone(),
            track,
            seek,
        };
        let input_id = self.input_ref.to_string();
        let handle = std::thread::Builder::new()
            .name("mp4 reader - audio".to_string())
            .spawn(move || {
                let _span = span!(Level::INFO, "MP4 audio", input_id = input_id).entered();
                track_thread.run_audio_thread()
            })
            .unwrap();
        (handle, shutdown_condition)
    }
}

struct TrackThread {
    ctx: TrackContext,
    shutdown_condition: ShutdownCondition,
    track: Track<File>,
    seek: Option<Duration>,
}

struct TrackThreadResult {
    track: Track<File>,
}

impl TrackThread {
    fn run_video_thread(mut self) -> TrackThreadResult {
        for (mut chunk, _duration) in self.track.chunks(self.seek) {
            self.ctx.stats_sender.send(
                Mp4InputTrackStatsEvent::BytesReceived(chunk.data.len())
                    .into_event(&self.ctx.input_ref, StatsTrackKind::Video),
            );

            // add buffer after recording last sample
            self.ctx.buffer.recalculate_buffer(chunk.pts);
            chunk.pts += self.ctx.buffer.size();

            trace!(pts=?chunk.pts, "MP4 reader produced a video chunk.");
            let chunk_sender = &self.ctx.decoder_handle.chunk_sender;
            if chunk_sender.send(PipelineEvent::Data(chunk)).is_err() {
                debug!("Failed to send a video chunk. Channel closed.");
                break;
            }

            if self.shutdown_condition.should_close() {
                break;
            }
        }
        let _ = self
            .ctx
            .event_sender
            .send(StateEvent::ThreadFinished(thread::current().id()));
        TrackThreadResult { track: self.track }
    }

    fn run_audio_thread(mut self) -> TrackThreadResult {
        for (mut chunk, _duration) in self.track.chunks(self.seek) {
            self.ctx.stats_sender.send(
                Mp4InputTrackStatsEvent::BytesReceived(chunk.data.len())
                    .into_event(&self.ctx.input_ref, StatsTrackKind::Audio),
            );

            // add buffer after recording last sample
            self.ctx.buffer.recalculate_buffer(chunk.pts);
            chunk.pts += self.ctx.buffer.size();

            trace!(pts=?chunk.pts, "MP4 reader produced a audio chunk.");
            let chunk_sender = &self.ctx.decoder_handle.chunk_sender;
            if chunk_sender.send(PipelineEvent::Data(chunk)).is_err() {
                debug!("Failed to send a audio chunk. Channel closed.");
                break;
            }

            if self.shutdown_condition.should_close() {
                break;
            }
        }
        let _ = self
            .ctx
            .event_sender
            .send(StateEvent::ThreadFinished(thread::current().id()));
        TrackThreadResult { track: self.track }
    }
}

struct SourceFile {
    path: Arc<Path>,
    remove_on_drop: bool,
}

impl Drop for SourceFile {
    fn drop(&mut self) {
        if self.remove_on_drop
            && let Err(e) = std::fs::remove_file(&self.path)
        {
            error!("Error while removing the downloaded mp4 file: {e}");
        }
    }
}
