use std::sync::Arc;

use compositor_render::InputId;
use tracing::{error, span, Level};

use crate::{
    error::InputInitError,
    pipeline::{decoder::DecodedDataReceiver, PipelineCtx},
};

use self::{capture::ChannelCallbackAdapter, find_device::find_decklink};

use super::{Input, InputInitInfo};

mod capture;
mod find_device;

pub use decklink::PixelFormat;

// sample rate returned from DeckLink
const AUDIO_SAMPLE_RATE: u32 = 48_000;

#[derive(Debug, thiserror::Error)]
pub enum DeckLinkError {
    #[error("Unknown DeckLink error.")]
    DecklinkError(#[from] decklink::DeckLinkError),
    #[error("No DeckLink device matches specified options. Found devices: {0:?}")]
    NoMatchingDeckLink(Vec<DeckLinkInfo>),
    #[error("Selected device does not support capture.")]
    NoCaptureSupport,
    #[error("Selected device does not support input format detection.")]
    NoInputFormatDetection,
}

#[derive(Debug)]
pub struct DeckLinkInfo {
    pub display_name: Option<String>,
    pub persistent_id: Option<String>,
    pub subdevice_index: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct DeckLinkOptions {
    pub subdevice_index: Option<u32>,
    pub display_name: Option<String>,
    /// Persistent id of a device (different value for each sub-device).
    pub persistent_id: Option<u32>,

    pub enable_audio: bool,
    /// Force specified pixel format, value resolved in input format
    /// autodetection will be ignored.
    pub pixel_format: Option<PixelFormat>,
}

pub struct DeckLink {
    input: Arc<decklink::Input>,
}

impl DeckLink {
    pub(super) fn new_input(
        ctx: Arc<PipelineCtx>,
        input_id: InputId,
        opts: DeckLinkOptions,
    ) -> Result<(Input, InputInitInfo, DecodedDataReceiver), InputInitError> {
        let span = span!(
            Level::INFO,
            "DeckLink input",
            input_id = input_id.to_string()
        );
        let input = Arc::new(
            find_decklink(&opts)?
                .input()
                .map_err(DeckLinkError::DecklinkError)?,
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
            .map_err(DeckLinkError::DecklinkError)?;
        input
            .enable_audio(AUDIO_SAMPLE_RATE, decklink::AudioSampleType::Sample32bit, 2)
            .map_err(DeckLinkError::DecklinkError)?;

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
            .map_err(DeckLinkError::DecklinkError)?;
        input
            .start_streams()
            .map_err(DeckLinkError::DecklinkError)?;

        Ok((
            Input::DeckLink(Self { input }),
            InputInitInfo::Other,
            DecodedDataReceiver {
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
