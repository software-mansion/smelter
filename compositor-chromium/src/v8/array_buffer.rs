use std::{ops::Deref, os::raw::c_void};

use crate::{
    cef_ref::{CefRc, CefRefData, CefStruct},
    validated::ValidatedError,
};

use super::{context::V8ContextEntered, value::V8Value};

pub struct V8ArrayBuffer(pub(super) CefRc<chromium_sys::cef_v8value_t>);

impl V8ArrayBuffer {
    /// Creates a new array buffer from raw pointer. It can be only created while in context.
    /// The buffer's memory is shared with V8 engine.
    ///
    /// Panics when used with CEF that was compiled with V8 sandbox support.
    ///
    /// # Safety
    /// Make sure the pointer is valid. Invalid pointer can cause undefined behavior.
    pub unsafe fn from_ptr(
        ptr: *mut u8,
        ptr_len: usize,
        _context_entered: &V8ContextEntered,
    ) -> Self {
        // We do not delete the buffer because it's not owned by this function
        let release_callback = V8ArrayBufferReleaseCallback::DoNotDelete;
        let inner = unsafe {
            chromium_sys::cef_v8value_create_array_buffer(
                ptr as *mut c_void,
                ptr_len,
                CefRefData::new_ptr(release_callback),
            )
        };

        Self(CefRc::new(inner))
    }

    /// Creates a new array buffer from raw pointer. The data is copied to the array buffer.
    /// It can be only created while in context.
    ///
    /// # Safety
    /// Make sure the pointer is valid. Invalid pointer can cause undefined behavior.
    pub unsafe fn from_ptr_with_copy(
        ptr: *mut u8,
        ptr_len: usize,
        _context_entered: &V8ContextEntered,
    ) -> Self {
        let inner = unsafe {
            chromium_sys::cef_v8value_create_array_buffer_with_copy(ptr as *mut c_void, ptr_len)
        };

        Self(CefRc::new(inner))
    }

    /// # Safety
    /// Make sure the pointer is valid. Invalid pointer can cause undefined behavior.
    pub unsafe fn update(
        &self,
        data: *mut u8,
        data_len: usize,
        _context_entered: &V8ContextEntered,
    ) -> Result<(), V8ArrayBufferError> {
        unsafe {
            let array_buffer = self.0.get_weak_with_validation()?;
            let get_array_buffer_len = (*array_buffer).get_array_buffer_byte_length.unwrap();
            let get_data = (*array_buffer).get_array_buffer_data.unwrap();

            if get_array_buffer_len(array_buffer) != data_len {
                return Err(V8ArrayBufferError::NotMatchingDataLength);
            }

            let buffer_data = get_data(array_buffer);
            std::ptr::copy(data as *mut _, buffer_data, data_len);
        }

        Ok(())
    }
}

impl From<V8ArrayBuffer> for V8Value {
    fn from(value: V8ArrayBuffer) -> Self {
        Self::ArrayBuffer(value)
    }
}

enum V8ArrayBufferReleaseCallback {
    #[allow(dead_code)]
    Delete {
        buffer_len: usize,
        buffer_cap: usize,
    },
    DoNotDelete,
}

impl CefStruct for V8ArrayBufferReleaseCallback {
    type CefType = chromium_sys::cef_v8array_buffer_release_callback_t;

    fn new_cef_data() -> Self::CefType {
        chromium_sys::cef_v8array_buffer_release_callback_t {
            base: unsafe { std::mem::zeroed() },
            release_buffer: Some(Self::release_buffer),
        }
    }

    fn base_from_cef_data(
        cef_data: &mut Self::CefType,
    ) -> &mut chromium_sys::cef_base_ref_counted_t {
        &mut cef_data.base
    }
}

impl V8ArrayBufferReleaseCallback {
    extern "C" fn release_buffer(
        self_: *mut chromium_sys::cef_v8array_buffer_release_callback_t,
        buffer: *mut c_void,
    ) {
        unsafe {
            let self_ref = CefRefData::<Self>::from_cef(self_);
            match (*self_ref).deref() {
                V8ArrayBufferReleaseCallback::Delete {
                    buffer_len,
                    buffer_cap,
                } => {
                    Vec::from_raw_parts(buffer, *buffer_len, *buffer_cap);
                }
                V8ArrayBufferReleaseCallback::DoNotDelete => {}
            };
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum V8ArrayBufferError {
    #[error("ArrayBuffer is not alive")]
    NotAlive(#[from] ValidatedError),

    #[error("V8Value is not an ArrayBuffer")]
    NotArrayBuffer,

    #[error("Provided data length is not the same as buffer length")]
    NotMatchingDataLength,
}
