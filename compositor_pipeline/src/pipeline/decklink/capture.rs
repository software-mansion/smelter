use std::{
    sync::{Mutex, OnceLock, Weak},
    time::{Duration, Instant},
};

use compositor_render::{error::ErrorStack, Frame, FrameData, Resolution};
use crossbeam_channel::{bounded, Receiver, Sender, TrySendError};
use decklink::{
    AudioInputPacket, DetectedVideoInputFormatFlags, DisplayMode, DisplayModeType, InputCallback,
    InputCallbackResult, PixelFormat, VideoInputFlags, VideoInputFormatChangedEvents,
    VideoInputFrame,
};
use tracing::{debug, info, trace, warn, Span};

use crate::pipeline::resampler::dynamic_resampler::{DynamicResampler, DynamicResamplerBatch};
use crate::prelude::*;

use super::AUDIO_SAMPLE_RATE;

pub(super) struct DataReceivers {
    pub(super) video: Option<Receiver<PipelineEvent<Frame>>>,
    pub(super) audio: Option<Receiver<PipelineEvent<InputAudioSamples>>>,
}

pub(super) struct ChannelCallbackAdapter {
    video_sender: Option<Sender<PipelineEvent<Frame>>>,
    audio_sender: Option<Sender<PipelineEvent<InputAudioSamples>>>,
    span: Span,

    // TODO 1: I'm not sure if we can get mutable reference to this adapter, so
    // wrapping it in mutex just in case
    // TODO 2: It's possible that we can just configure output sample rate in DeckLink
    // options instead of re-sampling ourselves.
    audio_resampler: Mutex<DynamicResampler>,

    // I'm not sure, but I suspect that holding Arc here would create a circular
    // dependency
    input: Weak<decklink::Input>,
    start_time: OnceLock<Instant>,
    offset: Mutex<Duration>,
    pixel_format: Option<PixelFormat>,
    last_format: Mutex<(DisplayModeType, PixelFormat)>,
}

impl ChannelCallbackAdapter {
    pub(super) fn new(
        span: Span,
        enable_audio: bool,
        output_sample_rate: u32,
        pixel_format: Option<PixelFormat>,
        input: Weak<decklink::Input>,
        initial_format: (DisplayModeType, PixelFormat),
    ) -> (Self, DataReceivers) {
        let (video_sender, video_receiver) = bounded(1000);
        let (audio_sender, audio_receiver) = match enable_audio {
            true => {
                let (sender, receiver) = bounded(1000);
                (Some(sender), Some(receiver))
            }
            false => (None, None),
        };
        (
            Self {
                video_sender: Some(video_sender),
                audio_sender,
                span,
                audio_resampler: Mutex::new(DynamicResampler::new(output_sample_rate)),
                input,
                start_time: OnceLock::new(),
                offset: Mutex::new(Duration::ZERO),
                pixel_format,
                last_format: Mutex::new(initial_format),
            },
            DataReceivers {
                video: Some(video_receiver),
                audio: audio_receiver,
            },
        )
    }

    fn start_time(&self) -> Instant {
        *self.start_time.get_or_init(Instant::now)
    }

    fn handle_video_frame(
        &self,
        video_frame: &mut VideoInputFrame,
        sender: &Sender<PipelineEvent<Frame>>,
    ) -> Result<(), decklink::DeckLinkError> {
        let offset = *self.offset.lock().unwrap();
        let pts = video_frame.stream_time()? + offset;

        let width = video_frame.width();
        let height = video_frame.height();
        let bytes_per_row = video_frame.bytes_per_row();
        let data = video_frame.bytes()?;
        let pixel_format = video_frame.pixel_format()?;

        let frame = match pixel_format {
            PixelFormat::Format8BitYUV => {
                Self::frame_from_yuv_422(width, height, bytes_per_row, data, pts)
            }
            // TODO just for testing
            PixelFormat::Format10BitRGB => {
                warn!(?pixel_format, "Unsupported pixel format");
                Self::frame_from_yuv_422(width, height, bytes_per_row, data, pts)
            }
            pixel_format => {
                warn!(?pixel_format, "Unsupported pixel format");
                return Ok(());
            }
        };

        trace!(?frame, ?pixel_format, "Received frame from decklink");
        match sender.try_send(PipelineEvent::Data(frame)) {
            Ok(_) => (),
            Err(TrySendError::Full(_)) => {
                warn!(
                    "Failed to send frame from DeckLink. Channel is full, dropping frame pts={pts:?}."
                )
            }
            Err(TrySendError::Disconnected(_)) => {
                debug!("Failed to send frame from DeckLink. Channel closed.");
            }
        }
        Ok(())
    }

    fn frame_from_yuv_422(
        width: usize,
        height: usize,
        bytes_per_row: usize,
        data: bytes::Bytes,
        pts: Duration,
    ) -> Frame {
        let data = if width != bytes_per_row * 2 {
            let mut output_buffer = bytes::BytesMut::with_capacity(width * 2 * height);

            data.chunks(bytes_per_row)
                .map(|chunk| &chunk[..(width * 2)])
                .for_each(|chunk| output_buffer.extend_from_slice(chunk));

            output_buffer.freeze()
        } else {
            data
        };
        Frame {
            data: FrameData::InterleavedYuv422(data),
            resolution: Resolution { width, height },
            pts,
        }
    }

    fn handle_audio_packet(
        &self,
        audio_packet: &mut AudioInputPacket,
        sender: &Sender<PipelineEvent<InputAudioSamples>>,
    ) -> Result<(), decklink::DeckLinkError> {
        let offset = *self.offset.lock().unwrap();
        let pts = audio_packet.packet_time()? + offset;

        let samples = audio_packet.as_32_bit_stereo()?;
        let samples = DynamicResamplerBatch {
            samples: AudioSamples::Stereo(
                samples
                    .into_iter()
                    .map(|(l, r)| (l as f64 / i32::MAX as f64, r as f64 / i32::MAX as f64))
                    .collect(),
            ),
            start_pts: pts,
            sample_rate: AUDIO_SAMPLE_RATE,
        };
        trace!(?samples, "Received audio samples from decklink");
        let resampled = self.audio_resampler.lock().unwrap().resample(samples);
        let resampled = match resampled {
            Ok(resampled) => resampled,
            Err(err) => {
                warn!("Resampler error: {}", ErrorStack::new(&err).into_string());
                return Ok(());
            }
        };
        for batch in resampled {
            match sender.try_send(PipelineEvent::Data(batch.into())) {
                Ok(_) => (),
                Err(TrySendError::Full(_)) => {
                    warn!(
                    "Failed to send samples from DeckLink. Channel is full, dropping samples pts={pts:?}."
                )
                }
                Err(TrySendError::Disconnected(_)) => {
                    debug!("Failed to send samples from DeckLink. Channel closed.")
                }
            }
        }
        Ok(())
    }

    fn handle_format_change(
        &self,
        display_mode: DisplayMode,
        flags: DetectedVideoInputFormatFlags,
    ) -> Result<(), decklink::DeckLinkError> {
        let Some(input) = self.input.upgrade() else {
            return Ok(());
        };

        let mode = display_mode.display_mode_type()?;

        let detected_pixel_format = if flags.format_y_cb_cr_422 {
            if flags.bit_depth_8 {
                PixelFormat::Format8BitYUV
            } else if flags.bit_depth_10 {
                PixelFormat::Format10BitYUV
            } else {
                warn!("Unknown format, falling back to 8-bit YUV");
                PixelFormat::Format8BitYUV
            }
        } else if flags.format_rgb_444 {
            if flags.bit_depth_8 {
                PixelFormat::Format8BitBGRA
            } else if flags.bit_depth_10 {
                PixelFormat::Format10BitRGB
            } else if flags.bit_depth_12 {
                PixelFormat::Format12BitRGB
            } else {
                warn!("Unknown format, falling back to 10-bit RGB");
                PixelFormat::Format10BitRGB
            }
        } else {
            warn!("Unknown format, skipping change");
            return Ok(());
        };

        let pixel_format = self.pixel_format.unwrap_or(detected_pixel_format);
        let (last_display_mode, last_pixel_format) = *self.last_format.lock().unwrap();
        if pixel_format == last_pixel_format && mode == last_display_mode {
            // skip if format is the same, otherwise this callback will be triggered
            // in the loop
            return Ok(());
        }

        *self.last_format.lock().unwrap() = (mode, pixel_format);

        info!("Detected new input format {mode:?} {detected_pixel_format:?} {flags:?}");

        if detected_pixel_format != pixel_format {
            info!(
                ?detected_pixel_format,
                ?pixel_format,
                "Specified pixel format does not match what was detected. Using {pixel_format:?}"
            );
        }

        input.pause_streams()?;
        input.enable_video(
            mode,
            pixel_format,
            VideoInputFlags {
                enable_format_detection: true,
                ..Default::default()
            },
        )?;
        input.flush_streams()?;
        input.start_streams()?;

        *self.offset.lock().unwrap() = self.start_time().elapsed();

        Ok(())
    }
}

impl InputCallback for ChannelCallbackAdapter {
    fn video_input_frame_arrived(
        &self,
        video_frame: Option<&mut VideoInputFrame>,
        audio_packet: Option<&mut AudioInputPacket>,
    ) -> InputCallbackResult {
        let _span = self.span.enter();

        // ensure init start time on first frame
        self.start_time();

        if let (Some(video_frame), Some(sender)) = (video_frame, &self.video_sender) {
            if let Err(err) = self.handle_video_frame(video_frame, sender) {
                warn!(
                    "Failed to handle video frame: {}",
                    ErrorStack::new(&err).into_string()
                )
            }
        }

        if let (Some(audio_packet), Some(sender)) = (audio_packet, &self.audio_sender) {
            if let Err(err) = self.handle_audio_packet(audio_packet, sender) {
                warn!(
                    "Failed to handle video frame: {}",
                    ErrorStack::new(&err).into_string()
                )
            }
        }

        InputCallbackResult::Ok
    }

    fn video_input_format_changed(
        &self,
        events: VideoInputFormatChangedEvents,
        display_mode: DisplayMode,
        flags: DetectedVideoInputFormatFlags,
    ) -> InputCallbackResult {
        let _span = self.span.enter();

        if events.field_dominance_changed
            || events.display_mode_changed
            || events.colorspace_changed
        {
            if let Err(err) = self.handle_format_change(display_mode, flags) {
                warn!(
                    "Failed to handle format change: {}",
                    ErrorStack::new(&err).into_string()
                );
            }
        }

        InputCallbackResult::Ok
    }
}
