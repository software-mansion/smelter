use std::sync::Arc;

use smelter_render::FramePreProcessor;
use tracing::{Level, error, span};

use crate::pipeline::input::Input;
use crate::queue::{InputSideChannel, QueueTrackOffset, QueueTrackOptions};
use crate::{pipeline::decklink::format::Format, queue::QueueInput};

use crate::prelude::*;

use self::{
    capture::{ChannelCallbackAdapter, PreProcessing},
    find_device::find_decklink,
    zero_copy::ZeroCopy,
};

mod capture;
mod find_device;
mod format;
mod zero_copy;

// sample rate returned from DeckLink
const AUDIO_SAMPLE_RATE: u32 = 48_000;

/// DeckLink input - captures raw video (and optionally audio) from a Blackmagic
/// DeckLink capture card via the DeckLink SDK callback interface.
///
/// ## Timestamps
///
/// - Register track with `QueueTrackOffset::Pts(Duration::ZERO)` which means
///   that PTS should be relative to queue `sync_point`.
/// - Video and audio use one offset because their timestamps share the card clock.
/// - Video-only capture has no presentation delay. Audio capture adds the same
///   40ms delay to both media to preserve A/V alignment.
/// - Never block on sending. Frames/samples are dropped if the channel is full.
///
/// ### Format detection
/// - Initial video mode is provisional (HD720p50). `enable_format_detection` is set,
///   so the SDK calls `video_input_format_changed` when the real format is detected.
/// - On format change, streams are paused, video is re-enabled with the new mode,
///   streams are flushed and restarted, and the stream offset is reset (recomputed
///   on the next packet).
///
/// ### Unsupported scenarios
/// - If ahead of time processing is enabled, initial registration will happen on pts already
///   processed by the queue, but queue will wait and eventually stream will show up, with
///   the portion at the start cut off.
/// - If queue is to slow (e.g. other input required and to slow), media will be delivered to
///   queue too late and dropped
pub struct DeckLink {
    input: Arc<decklink::Input>,
}

impl DeckLink {
    pub(super) fn new_input(
        ctx: Arc<PipelineCtx>,
        input_ref: Ref<InputId>,
        opts: DeckLinkInputOptions,
    ) -> Result<(Input, InputInitInfo, QueueInput), InputInitError> {
        let span = span!(
            Level::INFO,
            "DeckLink input",
            input_id = input_ref.to_string()
        );
        let input = Arc::new(
            find_decklink(&opts)?
                .input()
                .map_err(DeckLinkInputError::DecklinkError)?,
        );
        let initial_mode = decklink::DisplayModeType::ModeHD720p50;
        let initial_pixel_format = opts
            .pixel_format
            .unwrap_or(decklink::PixelFormat::Format8BitYUV);

        input
            .enable_audio(AUDIO_SAMPLE_RATE, decklink::AudioSampleType::Sample32bit, 2)
            .map_err(DeckLinkInputError::DecklinkError)?;

        let side_channel_enabled =
            opts.queue_options.video_side_channel != InputSideChannel::Disabled;
        let zero_copy = match opts.zero_copy {
            false => None,
            true if !side_channel_enabled => Err(DeckLinkInputError::ZeroCopyWithoutSideChannel)?,
            true if zero_copy::buffer_format(initial_pixel_format).is_none() => Err(
                DeckLinkInputError::ZeroCopyUnsupportedPixelFormat(initial_pixel_format),
            )?,
            true => Some(ZeroCopy::new(&ctx.wgpu_ctx.device)?),
        };
        let pre_processing = side_channel_enabled.then(|| PreProcessing {
            pre_processor: FramePreProcessor::new(ctx.wgpu_ctx.clone()),
            zero_copy,
        });

        let queue_input = QueueInput::new(&ctx, &input_ref, opts.queue_options);
        queue_input.set_stale_frame_timeout(ctx.stale_frame_timeout);
        let (video_sender, audio_sender) = queue_input.queue_new_track(QueueTrackOptions {
            video: true,
            audio: opts.enable_audio,
            offset: QueueTrackOffset::Pts(Timestamp::ZERO),
        });
        let callback = ChannelCallbackAdapter::new(
            &ctx,
            span,
            video_sender,
            audio_sender,
            pre_processing,
            Arc::<decklink::Input>::downgrade(&input),
            Format::new(initial_mode, initial_pixel_format),
        );
        callback
            .enable_video(&input, initial_mode, initial_pixel_format)
            .map_err(DeckLinkInputError::DecklinkError)?;
        input
            .set_callback(Box::new(callback))
            .map_err(DeckLinkInputError::DecklinkError)?;
        input
            .start_streams()
            .map_err(DeckLinkInputError::DecklinkError)?;

        Ok((
            Input::DeckLink(Self { input }),
            InputInitInfo::Other,
            queue_input,
        ))
    }
}

impl Drop for DeckLink {
    fn drop(&mut self) {
        if let Err(err) = self.input.stop_streams() {
            error!("Failed to stop streams: {:?}", err);
        }
    }
}
