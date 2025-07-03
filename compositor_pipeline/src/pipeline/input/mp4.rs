use std::{
    fs::File,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    thread::JoinHandle,
    time::Duration,
};

use compositor_render::InputId;
use crossbeam_channel::{bounded, Sender};
use reader::{DecoderOptions, Mp4FileReader, Track};
use tracing::{debug, error, span, trace, Level, Span};

use crate::{
    pipeline::{
        decoder::{AacDecoderOptions, AudioDecoderOptions, VideoDecoderOptions},
        EncodedChunk, VideoDecoder,
    },
    queue::PipelineEvent,
};

use super::{AudioInputReceiver, Input, InputInitInfo, InputInitResult, VideoInputReceiver};

pub mod reader;

#[derive(Debug, Clone)]
pub struct Mp4Options {
    pub source: Source,
    pub should_loop: bool,
    pub video_decoder: VideoDecoder,
}

#[derive(Debug, Clone)]
pub enum Source {
    Url(String),
    File(PathBuf),
}

#[derive(Debug, thiserror::Error)]
pub enum Mp4Error {
    #[error("Error while doing file operations.")]
    IoError(#[from] std::io::Error),

    #[error("Error while downloading the MP4 file.")]
    HttpError(#[from] reqwest::Error),

    #[error("Mp4 reader error.")]
    Mp4ReaderError(#[from] mp4::Error),

    #[error("No suitable track in the mp4 file")]
    NoTrack,

    #[error("Unknown error: {0}")]
    Unknown(&'static str),
}

pub struct Mp4 {
    should_close: Arc<AtomicBool>,
}

enum TrackType {
    Video,
    Audio,
}

impl Mp4 {
    pub(super) fn start_new_input(
        input_id: &InputId,
        options: Mp4Options,
        download_dir: &Path,
    ) -> Result<InputInitResult, Mp4Error> {
        let source = match options.source {
            Source::Url(ref url) => {
                let file_response = reqwest::blocking::get(url)?;
                let mut file_response = file_response.error_for_status()?;

                let mut path = download_dir.to_owned();
                path.push(format!("smelter-user-file-{}.mp4", rand::random::<u64>()));

                let mut file = std::fs::File::create(&path)?;

                std::io::copy(&mut file_response, &mut file)?;

                Arc::new(SourceFile {
                    path,
                    remove_on_drop: true,
                })
            }
            Source::File(ref path) => Arc::new(SourceFile {
                path: path.clone(),
                remove_on_drop: false,
            }),
        };

        let video = Mp4FileReader::from_path(&source.path)?.find_h264_track();
        let video_duration = video.as_ref().and_then(|track| track.duration());
        let audio = Mp4FileReader::from_path(&source.path)?.find_aac_track();
        let audio_duration = audio.as_ref().and_then(|track| track.duration());

        if video.is_none() && audio.is_none() {
            return Err(Mp4Error::NoTrack);
        }

        let (video_sender, video_receiver, video_track) = match video {
            Some(track) => {
                let (sender, receiver) = crossbeam_channel::bounded(10);
                let receiver = VideoInputReceiver::Encoded {
                    chunk_receiver: receiver,
                    decoder_options: match track.decoder_options() {
                        DecoderOptions::H264 => VideoDecoderOptions {
                            decoder: options.video_decoder,
                        },
                        _ => return Err(Mp4Error::Unknown("Non H264 decoder options returned.")),
                    },
                };
                (Some(sender), Some(receiver), Some(track))
            }
            None => (None, None, None),
        };

        let (audio_sender, audio_receiver, audio_track) = match audio {
            Some(track) => {
                let (sender, receiver) = crossbeam_channel::bounded(10);
                let receiver = AudioInputReceiver::Encoded {
                    chunk_receiver: receiver,
                    decoder_options: match track.decoder_options() {
                        DecoderOptions::Aac(data) => AudioDecoderOptions::Aac(AacDecoderOptions {
                            depayloader_mode: None,
                            asc: Some(data.clone()),
                        }),
                        _ => return Err(Mp4Error::Unknown("Non AAC decoder options returned.")),
                    },
                };
                (Some(sender), Some(receiver), Some(track))
            }
            None => (None, None, None),
        };

        let video_span = span!(Level::INFO, "MP4 video", input_id = input_id.to_string());
        let audio_span = span!(Level::INFO, "MP4 audio", input_id = input_id.to_string());
        let should_close = Arc::new(AtomicBool::new(false));
        if options.should_loop {
            start_thread_with_loop(
                video_sender,
                video_track,
                video_span,
                audio_sender,
                audio_track,
                audio_span,
                should_close.clone(),
                source,
            );
        } else {
            start_thread_single_run(
                video_sender,
                video_track,
                video_span,
                audio_sender,
                audio_track,
                audio_span,
                should_close.clone(),
                source,
            );
        }

        Ok(InputInitResult {
            input: Input::Mp4(Self { should_close }),
            video: video_receiver,
            audio: audio_receiver,
            init_info: InputInitInfo::Mp4 {
                video_duration,
                audio_duration,
            },
        })
    }
}

#[allow(clippy::too_many_arguments)]
fn start_thread_with_loop(
    video_sender: Option<Sender<PipelineEvent<EncodedChunk>>>,
    video_track: Option<Track<File>>,
    video_span: Span,
    audio_sender: Option<Sender<PipelineEvent<EncodedChunk>>>,
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
            let mut offset = Duration::ZERO;
            let has_audio = audio_track.is_some();
            let last_audio_sample_pts = Arc::new(AtomicU64::new(0));
            let last_video_sample_pts = Arc::new(AtomicU64::new(0));
            let mut video_track = video_track.map(|t| TrackProvider::Value(t.into()));
            let mut audio_track = audio_track.map(|t| TrackProvider::Value(t.into()));

            loop {
                let (finished_track_sender, finished_track_receiver) = bounded(1);
                let should_close = Arc::new(AtomicBool::new(false));
                let video_thread = video_sender
                    .clone()
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
                    .clone()
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
    video_sender: Option<Sender<PipelineEvent<EncodedChunk>>>,
    video_track: Option<Track<File>>,
    video_span: Span,
    audio_sender: Option<Sender<PipelineEvent<EncodedChunk>>>,
    audio_track: Option<Track<File>>,
    audio_span: Span,
    should_close: Arc<AtomicBool>,
    _source_file: Arc<SourceFile>,
) {
    if let (Some(sender), Some(mut track)) = (video_sender, video_track) {
        let should_close = should_close.clone();
        std::thread::Builder::new()
            .name("mp4 reader - video".to_string())
            .spawn(move || {
                let _span = video_span.enter();
                for (chunk, _duration) in track.chunks() {
                    if sender.send(PipelineEvent::Data(chunk)).is_err() {
                        debug!("Failed to send a video chunk. Channel closed.")
                    }
                    if should_close.load(Ordering::Relaxed) {
                        break;
                    }
                }
                if sender.send(PipelineEvent::EOS).is_err() {
                    debug!("Failed to send EOS from MP4 video reader. Channel closed.");
                }
            })
            .unwrap();
    }

    if let (Some(sender), Some(mut track)) = (audio_sender, audio_track) {
        let should_close = should_close.clone();
        std::thread::Builder::new()
            .name("mp4 reader - audio".to_string())
            .spawn(move || {
                let _span = audio_span.enter();
                for (chunk, _duration) in track.chunks() {
                    if sender.send(PipelineEvent::Data(chunk)).is_err() {
                        debug!("Failed to send a audio chunk. Channel closed.")
                    }
                    if should_close.load(Ordering::Relaxed) {
                        break;
                    }
                }
                if sender.send(PipelineEvent::EOS).is_err() {
                    debug!("Failed to send EOS from MP4 audio reader. Channel closed.");
                }
            })
            .unwrap();
    };
}

impl Drop for Mp4 {
    fn drop(&mut self) {
        self.should_close.store(true, Ordering::Relaxed);
    }
}

struct SourceFile {
    pub path: PathBuf,
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
