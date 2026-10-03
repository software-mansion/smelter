use std::{
    sync::{Arc, Mutex, Weak},
    time::{Duration, Instant},
};

use crossbeam_channel::TrySendError;
use decklink::{
    AudioInputPacket, DetectedVideoInputFormatFlags, DisplayMode, InputCallback,
    InputCallbackResult, PixelFormat, VideoInputFlags, VideoInputFormatChangedEvents,
    VideoInputFrame,
};
use smelter_render::{FrameData, FramePreProcessor, Resolution, error::ErrorStack};
use tracing::{Span, debug, info, trace, warn};

use crate::pipeline::decklink::{
    format::{BitDepth, Colorspace, Format},
    zero_copy::ZeroCopy,
};
use crate::queue::QueueSender;

use crate::prelude::*;

use super::AUDIO_SAMPLE_RATE;

pub(super) struct ChannelCallbackAdapter {
    video_sender: Option<QueueSender<Frame>>,
    audio_sender: Option<QueueSender<InputAudioSamples>>,
    /// Only set when a side channel is enabled (avoids duplicated processing).
    pre_processing: Option<Mutex<PreProcessing>>,
    span: Span,

    // I'm not sure, but I suspect that holding Arc here would create a circular
    // dependency
    input: Weak<decklink::Input>,
    sync_point: Instant,
    stream_offset: Mutex<Option<Timestamp>>,
    last_format: Mutex<Format>,
}

pub(super) struct PreProcessing {
    pub pre_processor: FramePreProcessor,
    pub zero_copy: Option<ZeroCopy>,
}

impl ChannelCallbackAdapter {
    pub(super) fn new(
        ctx: &Arc<PipelineCtx>,
        span: Span,
        video_sender: Option<QueueSender<Frame>>,
        audio_sender: Option<QueueSender<InputAudioSamples>>,
        pre_processing: Option<PreProcessing>,
        input: Weak<decklink::Input>,
        initial_format: Format,
    ) -> Self {
        Self {
            video_sender,
            audio_sender,
            pre_processing: pre_processing.map(Mutex::new),
            span,
            input,
            sync_point: ctx.queue_ctx.sync_point,
            stream_offset: Mutex::new(None),
            last_format: Mutex::new(initial_format),
        }
    }

    /// Format detection reports the real format through
    /// `video_input_format_changed`, which enables it in turn.
    pub(super) fn enable_video(
        &self,
        input: &decklink::Input,
        mode: decklink::DisplayModeType,
        pixel_format: PixelFormat,
    ) -> Result<(), decklink::DeckLinkError> {
        input.enable_video(
            mode,
            pixel_format,
            VideoInputFlags {
                enable_format_detection: true,
                ..Default::default()
            },
            self.pre_processing.as_ref().and_then(|pre_processing| {
                let pre_processing = pre_processing.lock().unwrap();
                pre_processing.zero_copy.as_ref().map(ZeroCopy::allocator)
            }),
        )
    }

    fn handle_video_frame(
        &self,
        video_frame: &mut VideoInputFrame,
        sender: &QueueSender<Frame>,
    ) -> Result<(), decklink::DeckLinkError> {
        let stream_time = video_frame.stream_time()?;
        let offset = {
            let mut guard = self.stream_offset.lock().unwrap();
            *guard.get_or_insert_with(|| self.sync_point.timestamp_now() - stream_time)
        };
        let presentation_delay =
            Duration::from_millis(if self.audio_sender.is_some() { 40 } else { 0 });
        let pts = offset + stream_time + presentation_delay;

        let resolution = Resolution {
            width: video_frame.width(),
            height: video_frame.height(),
        };
        let pixel_format = video_frame.pixel_format()?;
        let frame = match &self.pre_processing {
            Some(pre_processing) => {
                let PreProcessing {
                    pre_processor,
                    zero_copy,
                } = &mut *pre_processing.lock().unwrap();
                let texture = match zero_copy {
                    Some(zero_copy) => zero_copy.process(pre_processor, video_frame, pixel_format),
                    None => Self::frame(video_frame, video_frame.bytes()?, pixel_format, pts)
                        .map(|frame| pre_processor.process_to_texture(frame.into(), None)),
                };
                let Some(texture) = texture else {
                    return Ok(());
                };
                Frame {
                    data: FrameData::Rgba8UnormWgpuTexture(texture),
                    resolution,
                    pts,
                }
            }
            None => match Self::frame(
                video_frame,
                // Queued frames outlive DeckLink's buffer.
                bytes::Bytes::copy_from_slice(&video_frame.bytes()?),
                pixel_format,
                pts,
            ) {
                Some(frame) => frame,
                None => return Ok(()),
            },
        };

        trace!(?frame, ?pixel_format, "Received frame from decklink");
        match sender.try_send(frame) {
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

    fn frame(
        video_frame: &VideoInputFrame,
        data: bytes::Bytes,
        pixel_format: PixelFormat,
        pts: Timestamp,
    ) -> Option<Frame> {
        let width = video_frame.width();
        let height = video_frame.height();
        let bytes_per_row = video_frame.bytes_per_row();
        Some(match pixel_format {
            PixelFormat::Format8BitYUV => {
                Self::frame_from_yuv_422(width, height, bytes_per_row, data, pts)
            }
            PixelFormat::Format8BitARGB => {
                Self::frame_from_argb(width, height, bytes_per_row, data, pts)
            }
            PixelFormat::Format8BitBGRA => {
                Self::frame_from_bgra(width, height, bytes_per_row, data, pts)
            }
            pixel_format => {
                warn!(?pixel_format, "Unsupported pixel format");
                return None;
            }
        })
    }

    fn frame_from_yuv_422(
        width: usize,
        height: usize,
        bytes_per_row: usize,
        data: bytes::Bytes,
        pts: Timestamp,
    ) -> Frame {
        let data = if width * 2 != bytes_per_row {
            let mut output_buffer = bytes::BytesMut::with_capacity(width * 2 * height);

            data.chunks(bytes_per_row)
                .map(|chunk| &chunk[..(width * 2)])
                .for_each(|chunk| output_buffer.extend_from_slice(chunk));

            output_buffer.freeze()
        } else {
            data
        };
        Frame {
            data: FrameData::InterleavedUyvy422(data),
            resolution: Resolution { width, height },
            pts,
        }
    }

    fn frame_from_argb(
        width: usize,
        height: usize,
        bytes_per_row: usize,
        data: bytes::Bytes,
        pts: Timestamp,
    ) -> Frame {
        let data = if width * 4 != bytes_per_row {
            let mut output_buffer = bytes::BytesMut::with_capacity(width * 4 * height);

            data.chunks(bytes_per_row)
                .map(|chunk| &chunk[..(width * 4)])
                .for_each(|chunk| output_buffer.extend_from_slice(chunk));

            output_buffer.freeze()
        } else {
            data
        };
        Frame {
            data: FrameData::Argb(data),
            resolution: Resolution { width, height },
            pts,
        }
    }

    fn frame_from_bgra(
        width: usize,
        height: usize,
        bytes_per_row: usize,
        data: bytes::Bytes,
        pts: Timestamp,
    ) -> Frame {
        let data = if width * 4 != bytes_per_row {
            let mut output_buffer = bytes::BytesMut::with_capacity(width * 4 * height);

            data.chunks(bytes_per_row)
                .map(|chunk| &chunk[..(width * 4)])
                .for_each(|chunk| output_buffer.extend_from_slice(chunk));

            output_buffer.freeze()
        } else {
            data
        };
        Frame {
            data: FrameData::Bgra(data),
            resolution: Resolution { width, height },
            pts,
        }
    }

    fn handle_audio_packet(
        &self,
        audio_packet: &mut AudioInputPacket,
        sender: &QueueSender<InputAudioSamples>,
    ) -> Result<(), decklink::DeckLinkError> {
        let packet_time = audio_packet.packet_time()?;
        let offset = {
            let mut guard = self.stream_offset.lock().unwrap();
            *guard.get_or_insert_with(|| self.sync_point.timestamp_now() - packet_time)
        };
        let pts = offset + packet_time + Duration::from_millis(40);

        let samples = audio_packet.as_32_bit_stereo()?;
        let samples = InputAudioSamples {
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
        match sender.try_send(samples) {
            Ok(_) => (),
            Err(TrySendError::Full(_)) => {
                warn!(
                    ?pts,
                    "Failed to send samples from DeckLink. Channel is full, dropping samples."
                )
            }
            Err(TrySendError::Disconnected(_)) => {
                debug!("Failed to send samples from DeckLink. Channel closed.")
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

        let new_format = Format::from_mode_change(mode, flags);
        let last_format = *self.last_format.lock().unwrap();
        if new_format == last_format {
            // skip if format is the same, otherwise this callback will be triggered
            // in the loop
            return Ok(());
        }
        *self.last_format.lock().unwrap() = new_format;

        let pixel_format = match new_format.colorspace {
            Colorspace::YCbCr422 => {
                if new_format.bit_depth != BitDepth::Depth8Bit {
                    warn!(
                        "Format changed to {:?}. Forcing 8-bit.",
                        new_format.bit_depth
                    )
                }
                PixelFormat::Format8BitYUV
            }
            Colorspace::RGB444 => {
                if new_format.bit_depth != BitDepth::Depth8Bit {
                    warn!(
                        "Format changed to {:?}. Forcing 8-bit.",
                        new_format.bit_depth
                    )
                }
                PixelFormat::Format8BitBGRA
            }
            Colorspace::Unknown => return Ok(()),
        };

        info!(?pixel_format, ?flags, ?mode, "Detected new input format");

        input.pause_streams()?;
        self.enable_video(&input, mode, pixel_format)?;
        input.flush_streams()?;
        input.start_streams()?;

        // it will reset on the next packet
        *self.stream_offset.lock().unwrap() = None;

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

        if let (Some(video_frame), Some(sender)) = (video_frame, &self.video_sender)
            && let Err(err) = self.handle_video_frame(video_frame, sender)
        {
            warn!(
                "Failed to handle video frame: {}",
                ErrorStack::new(&err).into_string()
            )
        }

        if let (Some(audio_packet), Some(sender)) = (audio_packet, &self.audio_sender)
            && let Err(err) = self.handle_audio_packet(audio_packet, sender)
        {
            warn!(
                "Failed to handle video frame: {}",
                ErrorStack::new(&err).into_string()
            )
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

        if (events.field_dominance_changed
            || events.display_mode_changed
            || events.colorspace_changed)
            && let Err(err) = self.handle_format_change(display_mode, flags)
        {
            warn!(
                "Failed to handle format change: {}",
                ErrorStack::new(&err).into_string()
            );
        }

        InputCallbackResult::Ok
    }
}
