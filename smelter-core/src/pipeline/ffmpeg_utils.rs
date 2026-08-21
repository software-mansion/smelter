use std::{collections::HashMap, slice};

use ffmpeg_next::{Dictionary, Stream, StreamMut, codec::encoder, ffi::AVCodecParameters};

#[derive(Debug, Default)]
pub(super) struct FfmpegOptions(HashMap<String, String>);

impl FfmpegOptions {
    pub fn append<T: AsRef<str>>(&mut self, options: &[(T, T)]) {
        for (key, value) in options {
            self.0
                .insert(key.as_ref().to_string(), value.as_ref().to_string());
        }
    }

    pub fn into_dictionary(self) -> Dictionary<'static> {
        Dictionary::from_iter(self.0)
    }
}

impl<T: AsRef<str>, const N: usize> From<&[(T, T); N]> for FfmpegOptions {
    fn from(value: &[(T, T); N]) -> Self {
        let mut options = FfmpegOptions::default();
        options.append(value);
        options
    }
}

pub(super) fn write_extradata(codecpar: &mut AVCodecParameters, extradata: bytes::Bytes) {
    unsafe {
        // The allocated size of extradata must be at least extradata_size + AV_INPUT_BUFFER_PADDING_SIZE, with the padding bytes zeroed.
        codecpar.extradata = ffmpeg_next::ffi::av_mallocz(
            extradata.len() + ffmpeg_next::ffi::AV_INPUT_BUFFER_PADDING_SIZE as usize,
        ) as *mut u8;
        std::ptr::copy(extradata.as_ptr(), codecpar.extradata, extradata.len());
        codecpar.extradata_size = extradata.len() as i32;
    };
}

pub(crate) trait StreamMutExt {
    fn update_codecpar<F: FnOnce(&mut AVCodecParameters)>(&mut self, func: F);
}

impl StreamMutExt for StreamMut<'_> {
    fn update_codecpar<F: FnOnce(&mut AVCodecParameters)>(&mut self, func: F) {
        let codecpar = unsafe { &mut *(*self.as_mut_ptr()).codecpar };
        func(codecpar);
    }
}

/// Reads the codec configuration ffmpeg keeps as `extradata` (e.g. the AVC
/// decoder configuration record of an H.264 track).
pub(crate) trait ReadExtradataExt {
    /// Copies the extradata out; `None` when there is none.
    fn read_extradata(&self) -> Option<bytes::Bytes>;
}

impl ReadExtradataExt for Stream<'_> {
    fn read_extradata(&self) -> Option<bytes::Bytes> {
        unsafe {
            let codecpar = &*(*self.as_ptr()).codecpar;
            copy_extradata(codecpar.extradata, codecpar.extradata_size)
        }
    }
}

impl ReadExtradataExt for encoder::Video {
    fn read_extradata(&self) -> Option<bytes::Bytes> {
        unsafe {
            let encoder = &*self.0.0.0.as_ptr();
            copy_extradata(encoder.extradata, encoder.extradata_size)
        }
    }
}

/// # Safety
///
/// `extradata` has to point at `size` readable bytes.
unsafe fn copy_extradata(extradata: *const u8, size: i32) -> Option<bytes::Bytes> {
    match size > 0 {
        true => Some(bytes::Bytes::copy_from_slice(unsafe {
            slice::from_raw_parts(extradata, size as usize)
        })),
        false => None,
    }
}
