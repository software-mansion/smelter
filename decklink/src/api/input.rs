use std::{any::Any, ptr::null_mut, time::Duration};

use crate::{DeckLinkError, FrameAllocator, FrameBuffer, InputCallback, InputCallbackResult};

use super::{
    DisplayMode, HResult,
    ffi::{self, PixelFormat},
};

pub struct Input(pub(super) *mut ffi::IDeckLinkInput);

impl Input {
    pub fn supports_video_mode(
        &self,
        conn: ffi::VideoConnection,
        mode: ffi::DisplayModeType,
        pixel_format: ffi::PixelFormat,
        conversion_mode: ffi::VideoInputConversionMode,
        supported_mode_flags: ffi::SupportedVideoModeFlags,
    ) -> Result<(bool, ffi::DisplayModeType), DeckLinkError> {
        let mut is_supported = false;
        let mut actual_mode = ffi::DisplayModeType::ModeUnknown;
        unsafe {
            ffi::input_supports_video_mode(
                self.0,
                conn,
                mode,
                pixel_format,
                conversion_mode,
                supported_mode_flags,
                &mut actual_mode,
                &mut is_supported,
            )?
            .into_result("IDeckLinkInput::DoesSupportVideoMode")?;
        }
        Ok((is_supported, actual_mode))
    }
    /// Without an `allocator`, DeckLink captures into memory it allocates itself.
    pub fn enable_video(
        &self,
        mode: ffi::DisplayModeType,
        format: ffi::PixelFormat,
        flags: ffi::VideoInputFlags,
        allocator: Option<Box<dyn FrameAllocator>>,
    ) -> Result<(), DeckLinkError> {
        let result = match allocator {
            Some(allocator) => unsafe {
                ffi::input_enable_video_with_allocator(
                    self.0,
                    mode,
                    format,
                    flags,
                    Box::new(DynFrameAllocator(allocator)),
                )?
            },
            None => unsafe { ffi::input_enable_video(self.0, mode, format, flags)? },
        };
        match result {
            HResult::Ok => Ok(()),
            hresult => Err(DeckLinkError::DeckLinkCallFailed(
                "IDeckLinkInput::EnableVideoInput",
                hresult,
            )),
        }
    }
    pub fn enable_audio(
        &self,
        sample_rate: u32,
        sample_type: ffi::AudioSampleType,
        channels: u32,
    ) -> Result<(), DeckLinkError> {
        match unsafe { ffi::input_enable_audio(self.0, sample_rate, sample_type, channels)? } {
            HResult::Ok => Ok(()),
            hresult => Err(DeckLinkError::DeckLinkCallFailed(
                "IDeckLinkInput::EnableAudioInput",
                hresult,
            )),
        }
    }
    pub fn start_streams(&self) -> Result<(), DeckLinkError> {
        match unsafe { ffi::input_start_streams(self.0) } {
            HResult::Ok => Ok(()),
            hresult => Err(DeckLinkError::DeckLinkCallFailed(
                "IDeckLinkInput::StartStreams",
                hresult,
            )),
        }
    }
    pub fn stop_streams(&self) -> Result<(), DeckLinkError> {
        match unsafe { ffi::input_stop_streams(self.0) } {
            HResult::Ok => Ok(()),
            hresult => Err(DeckLinkError::DeckLinkCallFailed(
                "IDeckLinkInput::StopStreams",
                hresult,
            )),
        }
    }
    pub fn pause_streams(&self) -> Result<(), DeckLinkError> {
        match unsafe { ffi::input_pause_streams(self.0) } {
            HResult::Ok => Ok(()),
            hresult => Err(DeckLinkError::DeckLinkCallFailed(
                "IDeckLinkInput::PauseStreams",
                hresult,
            )),
        }
    }
    pub fn flush_streams(&self) -> Result<(), DeckLinkError> {
        match unsafe { ffi::input_flush_streams(self.0) } {
            HResult::Ok => Ok(()),
            hresult => Err(DeckLinkError::DeckLinkCallFailed(
                "IDeckLinkInput::FlushStreams",
                hresult,
            )),
        }
    }
    pub fn set_callback(&self, cb: Box<dyn InputCallback>) -> Result<(), DeckLinkError> {
        let cb = Box::new(DynInputCallback::new(cb));
        match unsafe { ffi::input_set_callback(self.0, cb) } {
            HResult::Ok => Ok(()),
            hresult => Err(DeckLinkError::DeckLinkCallFailed(
                "IDeckLinkInput::SetCallback",
                hresult,
            )),
        }
    }
}

impl Drop for Input {
    fn drop(&mut self) {
        unsafe { ffi::input_release(self.0) };
    }
}

unsafe impl Send for Input {}
unsafe impl Sync for Input {}

pub struct VideoInputFrame(*mut ffi::IDeckLinkVideoInputFrame);

impl VideoInputFrame {
    /// A view over DeckLink's own buffer, or a copy when an allocator lent
    /// the buffer: DeckLink captures into lent buffers again whatever
    /// references them.
    pub fn bytes(&self) -> Result<bytes::Bytes, DeckLinkError> {
        let len = self.height() * self.bytes_per_row();
        let access = unsafe { ffi::video_input_frame_start_access(self.0)? };
        let view = bytes::Bytes::from_owner(VideoBuffer { access, len });
        match unsafe { ffi::video_input_frame_buffer(self.0) }.is_null() {
            true => Ok(view),
            false => Ok(bytes::Bytes::copy_from_slice(&view)),
        }
    }
    /// The buffer the frame was captured into, when an allocator of type `B`
    /// lent it.
    pub fn buffer<B: FrameBuffer>(&self) -> Option<&B> {
        let buffer = unsafe { ffi::video_input_frame_buffer(self.0).as_ref()? };
        (buffer.0.as_ref() as &dyn Any).downcast_ref()
    }
    pub fn width(&self) -> usize {
        unsafe { ffi::video_input_frame_width(self.0) as usize }
    }
    pub fn height(&self) -> usize {
        unsafe { ffi::video_input_frame_height(self.0) as usize }
    }
    pub fn bytes_per_row(&self) -> usize {
        unsafe { ffi::video_input_frame_row_bytes(self.0) as usize }
    }
    pub fn pixel_format(&self) -> Result<PixelFormat, DeckLinkError> {
        Ok(unsafe { ffi::video_input_frame_pixel_format(self.0)? })
    }
    pub fn stream_time(&self) -> Result<Duration, DeckLinkError> {
        let time_value = unsafe { ffi::video_input_frame_stream_time(self.0, 1_000_000_000)? };
        Ok(Duration::from_nanos(time_value as u64))
    }
}

struct VideoBuffer {
    access: ffi::VideoBufferAccess,
    len: usize,
}

impl AsRef<[u8]> for VideoBuffer {
    fn as_ref(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.access.bytes, self.len) }
    }
}

impl Drop for VideoBuffer {
    fn drop(&mut self) {
        unsafe { ffi::video_buffer_end_access(self.access.buffer) };
    }
}

unsafe impl Send for VideoBuffer {}

pub struct AudioInputPacket(*mut ffi::IDeckLinkAudioInputPacket);

impl AudioInputPacket {
    pub fn raw_bytes(
        &self,
        channels: usize,
        sample_type: ffi::AudioSampleType,
    ) -> Result<bytes::Bytes, DeckLinkError> {
        let sample_count = unsafe { ffi::audio_input_packet_sample_count(self.0) as usize };
        let bytes_per_sample = sample_type.repr / 8;
        let bytes_len = sample_count * channels * bytes_per_sample as usize;
        let mut data = bytes::BytesMut::zeroed(bytes_len);
        unsafe {
            let packet_ptr = ffi::audio_input_packet_bytes(self.0)?;
            std::ptr::copy(packet_ptr, data.as_mut_ptr(), bytes_len);
        }
        Ok(data.freeze())
    }

    pub fn as_32_bit_stereo(&self) -> Result<Vec<(i32, i32)>, DeckLinkError> {
        let sample_count = unsafe { ffi::audio_input_packet_sample_count(self.0) as usize };
        let mut result: Vec<(i32, i32)> = Vec::with_capacity(sample_count);
        unsafe {
            let packet_ptr = ffi::audio_input_packet_bytes(self.0)?;
            let packet_ptr = packet_ptr as *mut [u8; 4];

            for index in 0..sample_count {
                let ch1 = *packet_ptr.add(index * 2);
                let ch2 = *packet_ptr.add(index * 2 + 1);
                result.push((i32::from_le_bytes(ch1), i32::from_le_bytes(ch2)));
            }
        }
        Ok(result)
    }

    pub fn as_16_bit_stereo(&self) -> Result<Vec<(i16, i16)>, DeckLinkError> {
        let sample_count = unsafe { ffi::audio_input_packet_sample_count(self.0) as usize };
        let mut result: Vec<(i16, i16)> = Vec::with_capacity(sample_count);
        unsafe {
            let packet_ptr = ffi::audio_input_packet_bytes(self.0)?;
            let packet_ptr = packet_ptr as *mut [u8; 2];

            for index in 0..sample_count {
                let ch1 = *packet_ptr.add(index * 2);
                let ch2 = *packet_ptr.add(index * 2 + 1);
                result.push((i16::from_le_bytes(ch1), i16::from_le_bytes(ch2)));
            }
        }
        Ok(result)
    }

    pub fn packet_time(&self) -> Result<Duration, DeckLinkError> {
        let time_value = unsafe { ffi::audio_input_packet_packet_time(self.0, 1_000_000_000)? };
        Ok(Duration::from_nanos(time_value as u64))
    }
}

pub(crate) struct DynFrameAllocator(Box<dyn FrameAllocator>);

impl DynFrameAllocator {
    pub(crate) fn allocate(&self, size: u32, row_bytes: u32) -> *mut DynFrameBuffer {
        self.0
            .allocate(size as usize, row_bytes as usize)
            .map_or(null_mut(), |buffer| {
                Box::into_raw(Box::new(DynFrameBuffer(buffer)))
            })
    }
}

pub(crate) struct DynFrameBuffer(Box<dyn FrameBuffer>);

impl DynFrameBuffer {
    pub(crate) fn bytes(&self) -> *mut u8 {
        self.0.bytes().as_ptr()
    }
}

pub(crate) struct DynInputCallback(Box<dyn InputCallback + 'static>);

impl DynInputCallback {
    fn new(cb: Box<dyn InputCallback + 'static>) -> DynInputCallback {
        DynInputCallback(cb)
    }
    pub(crate) unsafe fn video_input_frame_arrived(
        self: &DynInputCallback,
        video_frame: *mut ffi::IDeckLinkVideoInputFrame,
        audio_packet: *mut ffi::IDeckLinkAudioInputPacket,
    ) -> ffi::HResult {
        let mut video = None;
        if !video_frame.is_null() {
            video = Some(VideoInputFrame(video_frame))
        }

        let mut audio = None;
        if !audio_packet.is_null() {
            audio = Some(AudioInputPacket(audio_packet))
        }
        let result = self
            .0
            .video_input_frame_arrived(video.as_mut(), audio.as_mut());
        match result {
            InputCallbackResult::Ok => ffi::HResult::Ok,
            InputCallbackResult::Failure => ffi::HResult::Fail,
        }
    }

    pub(crate) fn video_input_format_changed(
        self: &DynInputCallback,
        events: ffi::VideoInputFormatChangedEvents,
        display_mode: *mut ffi::IDeckLinkDisplayMode,
        flags: ffi::DetectedVideoInputFormatFlags,
    ) -> ffi::HResult {
        let result =
            self.0
                .video_input_format_changed(events, DisplayMode(display_mode, false), flags);

        match result {
            InputCallbackResult::Ok => ffi::HResult::Ok,
            InputCallbackResult::Failure => ffi::HResult::Fail,
        }
    }
}
