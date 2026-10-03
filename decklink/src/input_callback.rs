use std::{any::Any, ptr::NonNull};

use crate::{
    AudioInputPacket, DisplayMode, VideoInputFrame,
    enums::ffi::{DetectedVideoInputFormatFlags, VideoInputFormatChangedEvents},
};

pub enum InputCallbackResult {
    Ok,
    Failure,
}

pub trait InputCallback {
    fn video_input_frame_arrived(
        &self,
        video_frame: Option<&mut VideoInputFrame>,
        audio_packet: Option<&mut AudioInputPacket>,
    ) -> InputCallbackResult;

    fn video_input_format_changed(
        &self,
        events: VideoInputFormatChangedEvents,
        display_mode: DisplayMode,
        flags: DetectedVideoInputFormatFlags,
    ) -> InputCallbackResult;
}

/// Supplies the buffers DeckLink captures one enabled format into. DeckLink
/// captures into its buffers in turn, whatever still references their frames,
/// and drops them when the format changes or capture stops.
pub trait FrameAllocator: Send + Sync {
    /// A buffer of `size` bytes holding rows of `row_bytes`. With `None`,
    /// DeckLink keeps capturing into the buffers it already holds, or fails to
    /// enable the format when it holds none.
    fn allocate(&self, size: usize, row_bytes: usize) -> Option<Box<dyn FrameBuffer>>;
}

/// Memory DeckLink captures a frame into, handed back by
/// `VideoInputFrame::buffer`.
///
/// # Safety
///
/// `bytes` must point to the `size` writable bytes requested from
/// `FrameAllocator::allocate`, at the same address for as long as the buffer
/// lives. DeckLink writes them while capturing, so nothing else may.
pub unsafe trait FrameBuffer: Any + Send + Sync {
    fn bytes(&self) -> NonNull<u8>;
}
