#[cfg(target_os = "linux")]
mod api;
#[cfg(target_os = "linux")]
mod enums;
#[cfg(target_os = "linux")]
mod info;
#[cfg(target_os = "linux")]
mod input_callback;

#[cfg(target_os = "linux")]
mod lib {
    use crate::api;
    use crate::enums;
    use crate::input_callback;

    pub use enums::ffi::FlagAttributeId;
    pub use enums::ffi::FloatAttributeId;
    pub use enums::ffi::IntegerAttributeId;
    pub use enums::ffi::StringAttributeId;

    pub use enums::ffi::AudioSampleType;
    pub use enums::ffi::DetectedVideoInputFormatFlags;
    pub use enums::ffi::DisplayModeType;
    pub use enums::ffi::PixelFormat;
    pub use enums::ffi::SupportedVideoModeFlags;
    pub use enums::ffi::VideoConnection;
    pub use enums::ffi::VideoIOSupport;
    pub use enums::ffi::VideoInputConversionMode;
    pub use enums::ffi::VideoInputFlags;
    pub use enums::ffi::VideoInputFormatChangedEvents;

    pub use api::DeckLink;
    pub use api::DisplayMode;
    pub use api::input::AudioInputPacket;
    pub use api::input::Input;
    pub use api::input::VideoInputFrame;
    pub use input_callback::InputCallback;
    pub use input_callback::InputCallbackResult;

    pub use api::get_decklinks;

    use api::HResult;

    #[derive(Debug, thiserror::Error)]
    pub enum DeckLinkError {
        #[error("Unknown error: {0:#}")]
        UnknownError(#[from] cxx::Exception),

        #[error("Method {0} failed with {1:?}")]
        DeckLinkCallFailed(&'static str, HResult),
    }

    impl From<i64> for VideoIOSupport {
        fn from(value: i64) -> Self {
            enums::ffi::into_video_io_support(value)
        }
    }
}

#[cfg(target_os = "linux")]
pub use lib::*;
