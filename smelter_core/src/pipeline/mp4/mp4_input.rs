use std::{
    fs::File,
    path::Path,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    thread::JoinHandle,
    time::Duration,
};

use crossbeam_channel::bounded;
use smelter_render::InputId;
use tracing::{debug, error, span, trace, Level, Span};

use crate::{
    pipeline::{
        decoder::{
            decoder_thread_audio::{AudioDecoderThread, AudioDecoderThreadOptions},
            decoder_thread_video::{VideoDecoderThread, VideoDecoderThreadOptions},
            fdk_aac, ffmpeg_h264,
            h264_utils::AvccToAnnexBRepacker,
            vulkan_h264, DecoderThreadHandle,
        },
        input::Input,
        mp4::reader::{DecoderOptions, Mp4FileReader, Track},
    },
    queue::QueueDataReceiver,
    thread_utils::InitializableThread,
};

use crate::prelude::*;

pub struct Mp4Input {
    should_close: Arc<AtomicBool>,
}

enum TrackType {
    Video,
    Audio,
}

impl Mp4Input {
    pub fn new_input(
        ctx: Arc<PipelineCtx>,
        input_id: InputId,
        options: Mp4InputOptions,
    ) -> Result<(Input, InputInitInfo, QueueDataReceiver), InputInitError> {
        let source = match options.source {
            Mp4InputSource::Url(url) => Self::download_remote_file(&ctx, &url)?,
            Mp4InputSource::File(path) => Arc::new(SourceFile {
                path,
                remove_on_drop: false,
            }),
        };

        let video = Mp4FileReader::from_path(&source.path)?.find_h264_track();
        let video_duration = video.as_ref().and_then(|track| track.duration());
        let audio = Mp4FileReader::from_path(&source.path)?.find_aac_track();
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

        let (video_handle, video_receiver, video_track) = match video {
            Some(track) => {
                let (sender, receiver) = crossbeam_channel::bounded(10);
                let handle = match (track.decoder_options(), h264_decoder) {
                    (DecoderOptions::H264(h264_config), VideoDecoderOptions::FfmpegH264) => {
                        VideoDecoderThread::<ffmpeg_h264::FfmpegH264Decoder, _>::spawn(
                            input_id.clone(),
                            VideoDecoderThreadOptions {
                                ctx: ctx.clone(),
                                transformer: Some(AvccToAnnexBRepacker::new(h264_config.clone())),
                                frame_sender: sender,
                                input_buffer_size: 5,
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
                            input_id.clone(),
                            VideoDecoderThreadOptions {
                                ctx: ctx.clone(),
                                transformer: Some(AvccToAnnexBRepacker::new(h264_config.clone())),
                                frame_sender: sender,
                                input_buffer_size: 5,
                            },
                        )?
                    }
                    _ => {
                        return Err(
                            Mp4InputError::Unknown("Non H264 decoder options returned.").into()
                        )
                    }
                };
                (Some(handle), Some(receiver), Some(track))
            }
            None => (None, None, None),
        };

        let (audio_handle, audio_receiver, audio_track) = match audio {
            Some(track) => {
                let (sender, receiver) = crossbeam_channel::bounded(10);
                let handle = match track.decoder_options() {
                    DecoderOptions::Aac(data) => {
                        AudioDecoderThread::<fdk_aac::FdkAacDecoder>::spawn(
                            input_id.clone(),
                            AudioDecoderThreadOptions {
                                ctx: ctx.clone(),
                                decoder_options: FdkAacDecoderOptions {
                                    asc: Some(data.clone()),
                                },
                                samples_sender: sender,
                                input_buffer_size: 5,
                            },
                        )?
                    }
                    _ => {
                        return Err(
                            Mp4InputError::Unknown("Non AAC decoder options returned.").into()
                        )
                    }
                };
                (Some(handle), Some(receiver), Some(track))
            }
            None => (None, None, None),
        };

        let video_span = span!(Level::INFO, "MP4 video", input_id = input_id.to_string());
        let audio_span = span!(Level::INFO, "MP4 audio", input_id = input_id.to_string());
        let should_close = Arc::new(AtomicBool::new(false));
        if options.should_loop {
            start_thread_with_loop(
                ctx.clone(),
                video_handle,
                video_track,
                video_span,
                audio_handle,
                audio_track,
                audio_span,
                should_close.clone(),
                source,
            );
        } else {
            start_thread_single_run(
                ctx.clone(),
                video_handle,
                video_track,
                video_span,
                audio_handle,
                audio_track,
                audio_span,
                should_close.clone(),
                source,
            );
        }

        Ok((
            Input::Mp4(Self { should_close }),
            InputInitInfo::Mp4 {
                video_duration,
                audio_duration,
            },
            QueueDataReceiver {
                video: video_receiver,
                audio: audio_receiver,
            },
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

#[allow(clippy::too_many_arguments)]
fn start_thread_with_loop(
    ctx: Arc<PipelineCtx>,
    video_handle: Option<DecoderThreadHandle>,
    video_track: Option<Track<File>>,
    video_span: Span,
    audio_sender: Option<DecoderThreadHandle>,
    audio_track: Option<Track<File>>,
    audio_span: Span,
    should_close_input: Arc<AtomicBool>,
    source_file: Arc<SourceFile>,
) {
    std::thread::Builder::new()
        .name("mp4 reader".to_string())
        .spawn(move || {
            enum TrackProvider {
                Value(Box<Track<File>>),
                Handle(JoinHandle<Box<Track<File>>>),
            }
            let _source_file = source_file;
            let mut offset = ctx.queue_sync_point.elapsed() + ctx.default_buffer_duration;
            let has_audio = audio_track.is_some();
            let last_audio_sample_pts = Arc::new(AtomicU64::new(0));
            let last_video_sample_pts = Arc::new(AtomicU64::new(0));
            let mut video_track = video_track.map(|t| TrackProvider::Value(t.into()));
            let mut audio_track = audio_track.map(|t| TrackProvider::Value(t.into()));

            loop {
                let (finished_track_sender, finished_track_receiver) = bounded(1);
                let should_close = Arc::new(AtomicBool::new(false));
                let video_thread = video_handle
                    .as_ref()
                    .map(|handle| handle.chunk_sender.clone())
                    .and_then(|sender| video_track.take().map(|track| (track, sender)))
                    .map(|(track, sender)| {
                        let span = video_span.clone();
                        let finished_track_sender = finished_track_sender.clone();
                        let last_sample_pts = last_video_sample_pts.clone();
                        let should_close = should_close.clone();
                        let should_close_input = should_close_input.clone();
                        std::thread::Builder::new()
                            .name("mp4 reader - video".to_string())
                            .spawn(move || {
                                let _span = span.enter();
                                let mut track = match track {
                                    TrackProvider::Value(track) => track,
                                    TrackProvider::Handle(handle) => handle.join().unwrap(),
                                };
                                for (mut chunk, duration) in track.chunks() {
                                    chunk.pts += offset;
                                    chunk.dts = chunk.dts.map(|dts| dts + offset);
                                    last_sample_pts.fetch_max(
                                        (chunk.pts + duration).as_nanos() as u64,
                                        Ordering::Relaxed,
                                    );
                                    trace!(pts=?chunk.pts, "MP4 reader produced a video chunk.");
                                    if sender.send(PipelineEvent::Data(chunk)).is_err() {
                                        debug!("Failed to send a video chunk. Channel closed.")
                                    }
                                    if should_close.load(Ordering::Relaxed)
                                        || should_close_input.load(Ordering::Relaxed)
                                    {
                                        break;
                                    }
                                    // TODO: send flush
                                }
                                let _ = finished_track_sender.send(TrackType::Video);
                                track
                            })
                            .unwrap()
                    });

                let audio_thread = audio_sender
                    .as_ref()
                    .map(|handle| handle.chunk_sender.clone())
                    .and_then(|sender| audio_track.take().map(|track| (track, sender)))
                    .map(|(track, sender)| {
                        let span = audio_span.clone();
                        let finished_track_sender = finished_track_sender.clone();
                        let last_sample_pts = last_audio_sample_pts.clone();
                        let should_close = should_close.clone();
                        let should_close_input = should_close_input.clone();
                        std::thread::Builder::new()
                            .name("mp4 reader - audio".to_string())
                            .spawn(move || {
                                let _span = span.enter();
                                let mut track = match track {
                                    TrackProvider::Value(track) => track,
                                    TrackProvider::Handle(handle) => handle.join().unwrap(),
                                };
                                for (mut chunk, duration) in track.chunks() {
                                    chunk.pts += offset;
                                    chunk.dts = chunk.dts.map(|dts| dts + offset);
                                    last_sample_pts.fetch_max(
                                        (chunk.pts + duration).as_nanos() as u64,
                                        Ordering::Relaxed,
                                    );
                                    trace!(pts=?chunk.pts, "MP4 reader produced an audio chunk.");
                                    if sender.send(PipelineEvent::Data(chunk)).is_err() {
                                        debug!("Failed to send a audio chunk. Channel closed.")
                                    }
                                    if should_close.load(Ordering::Relaxed)
                                        || should_close_input.load(Ordering::Relaxed)
                                    {
                                        break;
                                    }
                                    // TODO: send flush
                                }
                                let _ = finished_track_sender.send(TrackType::Audio);
                                track
                            })
                            .unwrap()
                    });

                match finished_track_receiver.recv().unwrap() {
                    TrackType::Video => {
                        video_track =
                            Some(TrackProvider::Value(video_thread.unwrap().join().unwrap()));
                        should_close.store(true, Ordering::Relaxed);
                        if let Some(audio_thread) = audio_thread {
                            audio_track = Some(TrackProvider::Handle(audio_thread));
                        }
                    }
                    TrackType::Audio => {
                        audio_track =
                            Some(TrackProvider::Value(audio_thread.unwrap().join().unwrap()));
                        should_close.store(true, Ordering::Relaxed);
                        if let Some(video_thread) = video_thread {
                            video_track = Some(TrackProvider::Handle(video_thread));
                        }
                    }
                }
                if has_audio {
                    offset = Duration::from_nanos(last_audio_sample_pts.load(Ordering::Relaxed));
                } else {
                    offset = Duration::from_nanos(last_video_sample_pts.load(Ordering::Relaxed));
                }
                if should_close_input.load(Ordering::Relaxed) {
                    return;
                }
            }
        })
        .unwrap();
}

#[allow(clippy::too_many_arguments)]
fn start_thread_single_run(
    ctx: Arc<PipelineCtx>,
    video_handle: Option<DecoderThreadHandle>,
    video_track: Option<Track<File>>,
    video_span: Span,
    audio_handle: Option<DecoderThreadHandle>,
    audio_track: Option<Track<File>>,
    audio_span: Span,
    should_close: Arc<AtomicBool>,
    _source_file: Arc<SourceFile>,
) {
    let offset = ctx.queue_sync_point.elapsed() + ctx.default_buffer_duration;
    if let (Some(handle), Some(mut track)) = (video_handle, video_track) {
        let should_close = should_close.clone();
        std::thread::Builder::new()
            .name("mp4 reader - video".to_string())
            .spawn(move || {
                let _span = video_span.enter();
                for (mut chunk, _duration) in track.chunks() {
                    chunk.pts += offset;
                    chunk.dts = chunk.dts.map(|dts| dts + offset);
                    trace!(?chunk, "Sending video chunk");
                    if handle
                        .chunk_sender
                        .send(PipelineEvent::Data(chunk))
                        .is_err()
                    {
                        debug!("Failed to send a video chunk. Channel closed.")
                    }
                    if should_close.load(Ordering::Relaxed) {
                        break;
                    }
                }
                if handle.chunk_sender.send(PipelineEvent::EOS).is_err() {
                    debug!("Failed to send EOS from MP4 video reader. Channel closed.");
                }
            })
            .unwrap();
    }

    if let (Some(handle), Some(mut track)) = (audio_handle, audio_track) {
        let should_close = should_close.clone();
        std::thread::Builder::new()
            .name("mp4 reader - audio".to_string())
            .spawn(move || {
                let _span = audio_span.enter();
                for (mut chunk, _duration) in track.chunks() {
                    chunk.pts += offset;
                    chunk.dts = chunk.dts.map(|dts| dts + offset);
                    trace!(?chunk, "Sending audio chunk");
                    if handle
                        .chunk_sender
                        .send(PipelineEvent::Data(chunk))
                        .is_err()
                    {
                        debug!("Failed to send a audio chunk. Channel closed.")
                    }
                    if should_close.load(Ordering::Relaxed) {
                        break;
                    }
                }
                if handle.chunk_sender.send(PipelineEvent::EOS).is_err() {
                    debug!("Failed to send EOS from MP4 audio reader. Channel closed.");
                }
            })
            .unwrap();
    };
}

impl Drop for Mp4Input {
    fn drop(&mut self) {
        self.should_close.store(true, Ordering::Relaxed);
    }
}

struct SourceFile {
    path: Arc<Path>,
    remove_on_drop: bool,
}

impl Drop for SourceFile {
    fn drop(&mut self) {
        if self.remove_on_drop {
            if let Err(e) = std::fs::remove_file(&self.path) {
                error!("Error while removing the downloaded mp4 file: {e}");
            }
        }
    }
}
