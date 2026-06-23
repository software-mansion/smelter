use std::ptr::null_mut;

use crate::DeckLinkError;

use self::{
    device::DeckLinkConfiguration,
    input::Input,
    profile::{ProfileAttributes, ProfileManager},
};
use input::DynInputCallback;

pub(super) mod device;
pub(super) mod input;
pub(super) mod profile;

#[cxx::bridge]
mod ffi {
    #[derive(Debug)]
    struct IDeckLinkPtr {
        ptr: *mut IDeckLink,
    }
    #[derive(Debug)]
    struct IDeckLinkProfilePtr {
        ptr: *mut IDeckLinkProfile,
    }

    #[derive(Debug)]
    struct Ratio {
        pub num: i64,
        pub den: i64,
    }

    // HResult is defined as C++ int, but values are larger than 32-bit integer
    // can hold, so we are using here u32 instead
    #[derive(Debug, Copy)]
    #[repr(u32)]
    pub enum HResult {
        /// S_OK
        Ok = 0x00000000,
        /// S_FALSE
        False = 0x00000001,
        /// E_UNEXPECTED
        UnexpectedError = 0x8000FFFF,
        /// E_NOTIMPL
        NotImplementedError = 0x80000001,
        /// E_OUTOFMEMORY
        OutOfMemoryError = 0x80000002,
        /// E_INVALIDARG
        InvalidArg = 0x80000003,
        /// E_NOINTERFACE
        NoInterfaceError = 0x80000004,
        /// E_POINTER - required out parameter is set to nullptr
        NullPointerError = 0x80000005,
        /// E_HANDLE
        HandleError = 0x80000006,
        /// E_ABORT
        Abort = 0x80000007,
        /// E_FAIL
        Fail = 0x80000008,
        /// E_ACCESSDENIED
        AccessDenied = 0x80000009,
    }

    extern "Rust" {
        pub type DynInputCallback;
        unsafe fn video_input_frame_arrived(
            self: &DynInputCallback,
            video_frame: *mut IDeckLinkVideoInputFrame,
            audio_packet: *mut IDeckLinkAudioInputPacket,
        ) -> HResult;
        unsafe fn video_input_format_changed(
            self: &DynInputCallback,
            events: VideoInputFormatChangedEvents,
            display_mode: *mut IDeckLinkDisplayMode,
            flags: DetectedVideoInputFormatFlags,
        ) -> HResult;
    }

    unsafe extern "C++" {
        include!("decklink/cpp/api.h");

        type FlagAttributeId = crate::enums::ffi::FlagAttributeId;
        type IntegerAttributeId = crate::enums::ffi::IntegerAttributeId;
        type FloatAttributeId = crate::enums::ffi::FloatAttributeId;
        type StringAttributeId = crate::enums::ffi::StringAttributeId;

        type FlagConfigurationId = crate::enums::ffi::FlagConfigurationId;
        type IntegerConfigurationId = crate::enums::ffi::IntegerConfigurationId;
        type FloatConfigurationId = crate::enums::ffi::FloatConfigurationId;
        type StringConfigurationId = crate::enums::ffi::StringConfigurationId;

        type VideoConnection = crate::enums::ffi::VideoConnection;
        type DisplayModeType = crate::enums::ffi::DisplayModeType;
        type PixelFormat = crate::enums::ffi::PixelFormat;
        type VideoInputConversionMode = crate::enums::ffi::VideoInputConversionMode;
        type VideoInputFormatChangedEvents =
            crate::enums::ffi::VideoInputFormatChangedEvents;
        type DetectedVideoInputFormatFlags =
            crate::enums::ffi::DetectedVideoInputFormatFlags;

        type SupportedVideoModeFlags = crate::enums::ffi::SupportedVideoModeFlags;
        type VideoInputFlags = crate::enums::ffi::VideoInputFlags;

        type AudioSampleType = crate::enums::ffi::AudioSampleType;

        type IDeckLink;
        type IDeckLinkInput;
        type IDeckLinkProfile;
        type IDeckLinkProfileManager;
        type IDeckLinkProfileAttributes;
        type IDeckLinkConfiguration;
        type IDeckLinkVideoInputFrame;
        type IDeckLinkVideoBuffer;
        type IDeckLinkAudioInputPacket;
        type IDeckLinkDisplayMode;

        fn get_decklinks() -> Result<Vec<IDeckLinkPtr>>;
    }

    // IDeckLink
    extern "C++" {
        unsafe fn decklink_profile_attributes(
            decklink: *mut IDeckLink,
            out: &mut *mut IDeckLinkProfileAttributes,
        ) -> HResult;
        unsafe fn decklink_input(
            decklink: *mut IDeckLink,
            out: &mut *mut IDeckLinkInput,
        ) -> HResult;
        unsafe fn decklink_profile_manager(
            decklink: *mut IDeckLink,
            out: &mut *mut IDeckLinkProfileManager,
        ) -> HResult;
        unsafe fn decklink_configuration(
            decklink: *mut IDeckLink,
            out: &mut *mut IDeckLinkConfiguration,
        ) -> HResult;
        unsafe fn decklink_release(decklink: *mut IDeckLink);
    }

    // IDeckLinkProfileAttributes
    extern "C++" {
        unsafe fn profile_attributes_flag(
            attrs: *mut IDeckLinkProfileAttributes,
            id: FlagAttributeId,
            out: &mut bool,
        ) -> Result<HResult>;
        unsafe fn profile_attributes_integer(
            attrs: *mut IDeckLinkProfileAttributes,
            id: IntegerAttributeId,
            out: &mut i64,
        ) -> Result<HResult>;
        unsafe fn profile_attributes_float(
            attrs: *mut IDeckLinkProfileAttributes,
            id: FloatAttributeId,
            out: &mut f64,
        ) -> Result<HResult>;
        unsafe fn profile_attributes_string(
            attrs: *mut IDeckLinkProfileAttributes,
            id: StringAttributeId,
            out: &mut String,
            is_static: bool,
        ) -> Result<HResult>;
        unsafe fn profile_attributes_release(attrs: *mut IDeckLinkProfileAttributes);
    }

    // InputRef
    extern "C++" {
        #[allow(clippy::too_many_arguments)]
        unsafe fn input_supports_video_mode(
            input: *mut IDeckLinkInput,
            conn: VideoConnection,
            mode: DisplayModeType,
            pixel_format: PixelFormat,
            conversion_mode: VideoInputConversionMode,
            supported_mode_flags: SupportedVideoModeFlags,
            out_mode: &mut DisplayModeType,
            out_supported: &mut bool,
        ) -> Result<HResult>;
        unsafe fn input_enable_video(
            input: *mut IDeckLinkInput,
            mode: DisplayModeType,
            format: PixelFormat,
            flags: VideoInputFlags,
        ) -> Result<HResult>;
        unsafe fn input_enable_audio(
            input: *mut IDeckLinkInput,
            sample_rate: u32,
            sample_type: AudioSampleType,
            channels: u32,
        ) -> Result<HResult>;
        unsafe fn input_start_streams(input: *mut IDeckLinkInput) -> HResult;
        unsafe fn input_stop_streams(input: *mut IDeckLinkInput) -> HResult;
        unsafe fn input_pause_streams(input: *mut IDeckLinkInput) -> HResult;
        unsafe fn input_flush_streams(input: *mut IDeckLinkInput) -> HResult;
        unsafe fn input_set_callback(
            input: *mut IDeckLinkInput,
            cb: Box<DynInputCallback>,
        ) -> HResult;

        unsafe fn input_release(input: *mut IDeckLinkInput);
    }

    // IDeckLinkProfileManager
    extern "C++" {
        unsafe fn profile_manager_profiles(
            manger: *mut IDeckLinkProfileManager,
            out: &mut Vec<IDeckLinkProfilePtr>,
        ) -> HResult;
        unsafe fn profile_manager_release(manager: *mut IDeckLinkProfileManager);
    }

    // IDeckLinkProfile
    extern "C++" {
        unsafe fn profile_profile_attributes(
            profile: *mut IDeckLinkProfile,
            out: &mut *mut IDeckLinkProfileAttributes,
        ) -> HResult;
        unsafe fn profile_is_active(
            profile: *mut IDeckLinkProfile,
            out: &mut bool,
        ) -> HResult;
        unsafe fn profile_release(profile: *mut IDeckLinkProfile);
    }

    // IDeckLinkConfiguration
    extern "C++" {
        unsafe fn configuration_flag(
            conf: *mut IDeckLinkConfiguration,
            id: FlagConfigurationId,
            out: &mut bool,
        ) -> Result<HResult>;
        unsafe fn configuration_integer(
            conf: *mut IDeckLinkConfiguration,
            id: IntegerConfigurationId,
            out: &mut i64,
        ) -> Result<HResult>;
        unsafe fn configuration_float(
            conf: *mut IDeckLinkConfiguration,
            id: FloatConfigurationId,
            out: &mut f64,
        ) -> Result<HResult>;
        unsafe fn configuration_string(
            conf: *mut IDeckLinkConfiguration,
            id: StringConfigurationId,
            out: &mut String,
        ) -> Result<HResult>;
        unsafe fn configuration_set_flag(
            conf: *mut IDeckLinkConfiguration,
            id: FlagConfigurationId,
            value: bool,
        ) -> Result<HResult>;
        unsafe fn configuration_set_integer(
            conf: *mut IDeckLinkConfiguration,
            id: IntegerConfigurationId,
            value: i64,
        ) -> Result<HResult>;
        unsafe fn configuration_set_float(
            conf: *mut IDeckLinkConfiguration,
            id: FloatConfigurationId,
            value: f64,
        ) -> Result<HResult>;
        unsafe fn configuration_set_string(
            conf: *mut IDeckLinkConfiguration,
            id: StringConfigurationId,
            value: String,
        ) -> Result<HResult>;
        unsafe fn configuration_release(conf: *mut IDeckLinkConfiguration);
    }

    // IDeckLinkVideoInputFrame
    extern "C++" {
        unsafe fn video_input_frame_height(input: *mut IDeckLinkVideoInputFrame) -> i64;
        unsafe fn video_input_frame_width(input: *mut IDeckLinkVideoInputFrame) -> i64;
        unsafe fn video_input_frame_row_bytes(
            input: *mut IDeckLinkVideoInputFrame,
        ) -> i64;
        unsafe fn video_input_frame_start_access(
            input: *mut IDeckLinkVideoInputFrame,
            buffer: &mut *mut IDeckLinkVideoBuffer,
        ) -> Result<*mut u8>;
        unsafe fn video_input_frame_end_access(buffer: *mut IDeckLinkVideoBuffer);
        unsafe fn video_input_frame_pixel_format(
            input: *mut IDeckLinkVideoInputFrame,
        ) -> Result<PixelFormat>;
        unsafe fn video_input_frame_stream_time(
            input: *mut IDeckLinkVideoInputFrame,
            time_scale: i64,
        ) -> Result<i64>;
    }

    // IDeckLinkAudioInputPacket
    extern "C++" {
        unsafe fn audio_input_packet_bytes(
            input: *mut IDeckLinkAudioInputPacket,
        ) -> Result<*mut u8>;
        unsafe fn audio_input_packet_sample_count(
            input: *mut IDeckLinkAudioInputPacket,
        ) -> i64;
        unsafe fn audio_input_packet_packet_time(
            input: *mut IDeckLinkAudioInputPacket,
            time_scale: i64,
        ) -> Result<i64>;
    }

    // IDeckLinkDisplayMode
    extern "C++" {
        unsafe fn display_mode_width(mode: *mut IDeckLinkDisplayMode) -> i64;
        unsafe fn display_mode_height(mode: *mut IDeckLinkDisplayMode) -> i64;
        unsafe fn display_mode_name(mode: *mut IDeckLinkDisplayMode) -> Result<String>;
        unsafe fn display_mode_display_mode_type(
            mode: *mut IDeckLinkDisplayMode,
        ) -> Result<DisplayModeType>;
        unsafe fn display_mode_frame_rate(
            mode: *mut IDeckLinkDisplayMode,
        ) -> Result<Ratio>;

        unsafe fn display_mode_release(mode: *mut IDeckLinkDisplayMode);
    }
}

pub use ffi::HResult;

pub struct DeckLink(*mut ffi::IDeckLink);

impl DeckLink {
    pub fn profile_attributes(&self) -> Result<ProfileAttributes, DeckLinkError> {
        let mut attr = null_mut();
        unsafe { ffi::decklink_profile_attributes(self.0, &mut attr) }.into_result(
            "IDeckLink::QueryInterface(IID_IDeckLinkProfileAttributes, _)",
        )?;
        Ok(ProfileAttributes(attr))
    }

    pub fn input(&self) -> Result<Input, DeckLinkError> {
        let mut input = null_mut();
        unsafe { ffi::decklink_input(self.0, &mut input) }
            .into_result("IDeckLink::QueryInterface(IID_IDeckLinkInput, _)")?;
        Ok(Input(input))
    }

    pub fn profile_manager(&self) -> Result<Option<ProfileManager>, DeckLinkError> {
        let mut manager = null_mut();
        let hresult = unsafe { ffi::decklink_profile_manager(self.0, &mut manager) };

        if HResult::NoInterfaceError == hresult {
            return Ok(None);
        }
        hresult
            .into_result("IDeckLink::QueryInterface(IID_IDeckLinkProfileManager, _)")?;

        Ok(Some(ProfileManager(manager)))
    }

    pub fn configuration(&self) -> Result<DeckLinkConfiguration, DeckLinkError> {
        let mut configuration = null_mut();
        unsafe { ffi::decklink_configuration(self.0, &mut configuration) }
            .into_result("IDeckLink::QueryInterface(IID_IDeckLinkConfiguration, _)")?;
        Ok(DeckLinkConfiguration(configuration))
    }
}

impl Drop for DeckLink {
    fn drop(&mut self) {
        unsafe { ffi::decklink_release(self.0) };
    }
}

pub fn get_decklinks() -> Result<Vec<DeckLink>, DeckLinkError> {
    let ptrs = ffi::get_decklinks()?;
    Ok(ptrs.into_iter().map(|wrapper| DeckLink(wrapper.ptr)).collect())
}

pub struct DisplayMode(*mut ffi::IDeckLinkDisplayMode, bool);

impl DisplayMode {
    pub fn width(&self) -> usize {
        unsafe { ffi::display_mode_width(self.0) as usize }
    }

    pub fn height(&self) -> usize {
        unsafe { ffi::display_mode_height(self.0) as usize }
    }

    pub fn name(&self) -> Result<String, DeckLinkError> {
        Ok(unsafe { ffi::display_mode_name(self.0) }?)
    }

    pub fn display_mode_type(&self) -> Result<ffi::DisplayModeType, DeckLinkError> {
        Ok(unsafe { ffi::display_mode_display_mode_type(self.0) }?)
    }

    pub fn display_mode_frame_rate(&self) -> Result<ffi::Ratio, DeckLinkError> {
        Ok(unsafe { ffi::display_mode_frame_rate(self.0) }?)
    }
}

impl Drop for DisplayMode {
    fn drop(&mut self) {
        if !self.1 {
            return;
        }
        unsafe { ffi::display_mode_release(self.0) };
    }
}

impl ffi::HResult {
    fn into_result(self, fn_name: &'static str) -> Result<(), DeckLinkError> {
        match self {
            ffi::HResult::Ok => Ok(()),
            hresult => Err(DeckLinkError::DeckLinkCallFailed(fn_name, hresult)),
        }
    }
}
