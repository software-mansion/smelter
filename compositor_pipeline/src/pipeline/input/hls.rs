use std::{
    ffi::CString,
    ptr, slice,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

use crate::{
    error::InputInitError,
    pipeline::{
        decoder::{AacDecoderOptions, AudioDecoderOptions, VideoDecoderOptions},
        types::{EncodedChunk, IsKeyframe},
        AudioCodec, EncodedChunkKind, VideoCodec, VideoDecoder,
    },
    queue::PipelineEvent,
};
use compositor_render::InputId;
use crossbeam_channel::{bounded, Sender};
use ffmpeg_next::{
    ffi::{
        avformat_alloc_context, avformat_close_input, avformat_find_stream_info,
        avformat_open_input,
    },
    format::context,
    media::Type,
    util::interrupt,
    Dictionary, Packet,
};
use tracing::{debug, span, warn, Level};

use super::{AudioInputReceiver, Input, InputInitInfo, InputInitResult, VideoInputReceiver};

#[derive(Debug, Clone)]
pub struct HlsInputOptions {
    pub url: Arc<str>,
}

pub struct HlsInput {
    should_close: Arc<AtomicBool>,
}

impl HlsInput {
    pub(super) fn start_new_input(
        input_id: &InputId,
        opts: HlsInputOptions,
    ) -> Result<InputInitResult, InputInitError> {
        let should_close = Arc::new(AtomicBool::new(false));

        let (video, audio) = Self::spawn_thread(input_id.clone(), should_close.clone(), opts)?;

        Ok(InputInitResult {
            input: Input::Hls(Self { should_close }),
            video,
            audio,
            init_info: InputInitInfo::Other,
        })
    }

    fn spawn_thread(
        input_id: InputId,
        should_close: Arc<AtomicBool>,
        options: HlsInputOptions,
    ) -> Result<(Option<VideoInputReceiver>, Option<AudioInputReceiver>), InputInitError> {
        let (result_sender, result_receiver) = bounded(1);
        std::thread::Builder::new()
            .name(format!("HLS thread for input {}", input_id.clone()))
            .spawn(move || {
                let _span =
                    span!(Level::INFO, "HLS thread", input_id = input_id.to_string()).entered();

                Self::run_thread(options, should_close, result_sender);
            })
            .unwrap();

        result_receiver.recv().unwrap()
    }

    fn run_thread(
        options: HlsInputOptions,
        should_close: Arc<AtomicBool>,
        result_sender: Sender<
            Result<(Option<VideoInputReceiver>, Option<AudioInputReceiver>), InputInitError>,
        >,
    ) {
        // careful: moving the input context in any way will cause ffmpeg to segfault
        // I do not know why this happens
        let mut input_ctx = match input_with_dictionary_and_interrupt(
            &options.url,
            Dictionary::from_iter([("protocol_whitelist", "tcp,hls,http,https,file,tls")]),
            || should_close.load(Ordering::Relaxed),
        ) {
            Ok(i) => i,
            Err(e) => {
                result_sender
                    .send(Err(InputInitError::FfmpegError(e)))
                    .unwrap();
                return;
            }
        };

        let (audio, audio_result) = match input_ctx.streams().best(Type::Audio) {
            Some(stream) => {
                // not tested it was always null, but audio is in ADTS, so config is not
                // necessary
                let config = unsafe {
                    let codecpar = (*stream.as_ptr()).codecpar;
                    let size = (*codecpar).extradata_size;
                    if size > 0 {
                        Some(bytes::Bytes::copy_from_slice(slice::from_raw_parts(
                            (*codecpar).extradata,
                            size as usize,
                        )))
                    } else {
                        None
                    }
                };
                let (sender, receiver) = bounded(1000);
                (
                    Some((stream.index(), stream.time_base(), sender)),
                    Some((receiver, config)),
                )
            }
            None => (None, None),
        };
        let (video, video_receiver) = match input_ctx.streams().best(Type::Video) {
            Some(stream) => {
                // warn!(
                //     "Video stream {:?}",
                //     (stream.time_base(), stream.start_time(), stream.metadata())
                // );
                let (sender, receiver) = bounded(1000);
                (
                    Some((stream.index(), stream.time_base(), sender)),
                    Some(receiver),
                )
            }
            None => (None, None),
        };

        let mut send_init_result = Some(move || {
            result_sender
                .send(Ok((
                    video_receiver.map(|video| VideoInputReceiver::Encoded {
                        chunk_receiver: video,
                        decoder_options: VideoDecoderOptions {
                            decoder: VideoDecoder::VulkanVideoH264,
                        },
                    }),
                    audio_result.map(|(receiver, asc)| AudioInputReceiver::Encoded {
                        chunk_receiver: receiver,
                        decoder_options: AudioDecoderOptions::Aac(AacDecoderOptions {
                            depayloader_mode: None,
                            asc,
                        }),
                    }),
                )))
                .unwrap()
        });

        let mut packet = Packet::empty();
        loop {
            match packet.read(&mut input_ctx) {
                Ok(_) => (),
                Err(ffmpeg_next::Error::Eof | ffmpeg_next::Error::Exit) => break,
                Err(err) => {
                    warn!("HLS read error {err:?}");
                    continue;
                }
            }

            if packet.flags().contains(ffmpeg_next::packet::Flags::CORRUPT) {
                debug!(
                    "Corrupted packet {:?} {:?}",
                    packet.stream(),
                    packet.flags()
                );
                continue;
            }

            if let Some((index, time_base, ref sender)) = video {
                if sender.len() > 600 {
                    send_init_result.take().map(|fun| fun());
                }
                if packet.stream() == index {
                    debug!(
                        "Video packet {:?}",
                        (packet.stream(), packet.pts(), sender.len(),)
                    );

                    let chunk = PipelineEvent::Data(EncodedChunk {
                        data: bytes::Bytes::copy_from_slice(packet.data().unwrap()),
                        pts: Duration::from_secs_f64(
                            packet.pts().unwrap_or(0) as f64 * time_base.0 as f64
                                / time_base.1 as f64,
                        ),
                        dts: packet.dts().map(|dts| {
                            Duration::from_secs_f64(
                                dts as f64 * time_base.0 as f64 / time_base.1 as f64,
                            )
                        }),
                        is_keyframe: IsKeyframe::Unknown,
                        kind: EncodedChunkKind::Video(VideoCodec::H264),
                    });
                    if sender.len() == 0 {
                        warn!("HLS input video channel was drained")
                    }
                    if sender.send(chunk).is_err() {
                        debug!("Channel closed")
                    }
                }
            }

            if let Some((index, time_base, ref sender)) = audio {
                if sender.len() > 600 {
                    send_init_result.take().map(|fun| fun());
                }
                if packet.stream() == index {
                    debug!(
                        "Audio packet {:?}",
                        (packet.stream(), packet.pts(), sender.len(),)
                    );
                    let chunk = PipelineEvent::Data(EncodedChunk {
                        data: bytes::Bytes::copy_from_slice(packet.data().unwrap()),
                        pts: Duration::from_secs_f64(
                            packet.pts().unwrap_or(0) as f64 * time_base.0 as f64
                                / time_base.1 as f64,
                        ),
                        dts: packet.dts().map(|dts| {
                            Duration::from_secs_f64(
                                dts as f64 * time_base.0 as f64 / time_base.1 as f64,
                            )
                        }),
                        is_keyframe: IsKeyframe::Unknown,
                        kind: EncodedChunkKind::Audio(AudioCodec::Aac),
                    });
                    if sender.len() == 0 {
                        warn!("HLS input audio channel was drained")
                    }
                    if sender.send(chunk).is_err() {
                        debug!("Channel closed")
                    }
                }
            }
        }

        // just to make sure init is finished for short streams
        send_init_result.take().map(|fun| fun());

        if let Some((_, _, sender)) = audio {
            if sender.send(PipelineEvent::EOS).is_err() {
                debug!("Failed to send EOS message.")
            }
        }

        if let Some((_, _, sender)) = video {
            if sender.send(PipelineEvent::EOS).is_err() {
                debug!("Failed to send EOS message.")
            }
        }
    }
}

impl Drop for HlsInput {
    fn drop(&mut self) {
        self.should_close
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

/// Combined implementation of ffmpeg_next::format:input_with_interrupt and
/// ffmpeg_next::format::input_with_dictionary that allows passing both interrupt
/// callback and Dictionary with options
pub fn input_with_dictionary_and_interrupt<F>(
    path: &str,
    options: Dictionary,
    closure: F,
) -> Result<context::Input, ffmpeg_next::Error>
where
    F: FnMut() -> bool,
{
    unsafe {
        let mut ps = avformat_alloc_context();

        (*ps).interrupt_callback = interrupt::new(Box::new(closure)).interrupt;

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
