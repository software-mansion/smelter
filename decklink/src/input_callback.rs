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
