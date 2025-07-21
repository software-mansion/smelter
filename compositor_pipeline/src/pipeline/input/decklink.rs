use std::sync::Arc;

use compositor_render::InputId;
use tracing::{error, span, Level};

use crate::prelude::*;
use crate::{pipeline::input::Input, queue::QueueDataReceiver};

use self::{capture::ChannelCallbackAdapter, find_device::find_decklink};

mod capture;
mod find_device;

// sample rate returned from DeckLink
const AUDIO_SAMPLE_RATE: u32 = 48_000;

pub struct DeckLink {
    input: Arc<decklink::Input>,
}

impl DeckLink {
    pub(super) fn new_input(
        ctx: Arc<PipelineCtx>,
        input_id: InputId,
        opts: DeckLinkInputOptions,
    ) -> Result<(Input, InputInitInfo, QueueDataReceiver), InputInitError> {
        let span = span!(
            Level::INFO,
            "DeckLink input",
            input_id = input_id.to_string()
        );
        let input = Arc::new(
            find_decklink(&opts)?
                .input()
                .map_err(DeckLinkInputError::DecklinkError)?,
        );

        // Initial options, real config should be set based on detected format, thanks
        // to the `enable_format_detection` option. When enabled it will call
        // `video_input_format_changed` method with a detected format.
        input
            .enable_video(
                decklink::DisplayModeType::ModeHD720p50,
                decklink::PixelFormat::Format8BitYUV,
                decklink::VideoInputFlags {
                    enable_format_detection: true,
                    ..Default::default()
                },
            )
            .map_err(DeckLinkInputError::DecklinkError)?;
        input
            .enable_audio(AUDIO_SAMPLE_RATE, decklink::AudioSampleType::Sample32bit, 2)
            .map_err(DeckLinkInputError::DecklinkError)?;

        let (callback, receivers) = ChannelCallbackAdapter::new(
            span,
            opts.enable_audio,
            ctx.mixing_sample_rate,
            opts.pixel_format,
            Arc::<decklink::Input>::downgrade(&input),
            (
                decklink::DisplayModeType::ModeHD720p50,
                decklink::PixelFormat::Format8BitYUV,
            ),
        );
        input
            .set_callback(Box::new(callback))
            .map_err(DeckLinkInputError::DecklinkError)?;
        input
            .start_streams()
            .map_err(DeckLinkInputError::DecklinkError)?;

        Ok((
            Input::DeckLink(Self { input }),
            InputInitInfo::Other,
            QueueDataReceiver {
                video: receivers.video,
                audio: receivers.audio,
            },
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
