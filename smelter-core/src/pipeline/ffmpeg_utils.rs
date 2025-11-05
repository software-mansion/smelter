use std::collections::HashMap;

use ffmpeg_next::{Dictionary, StreamMut, ffi::AVCodecParameters};

#[derive(Debug, Clone, Default)]
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
