use crate::cef_ref::CefRc;
use crate::validated::ValidatedError;

use super::value::{V8Value, V8ValueError};

pub struct V8Array(pub(super) CefRc<libcef_sys::cef_v8value_t>);

impl V8Array {
    pub fn new(len: usize) -> Self {
        let inner = unsafe { libcef_sys::cef_v8value_create_array(len as i32) };
        Self(CefRc::new(inner))
    }

    pub fn has(&self, index: usize) -> Result<bool, V8ArrayError> {
        let inner = self.0.get_weak_with_validation()?;
        unsafe {
            let has_value = (*inner).has_value_byindex.unwrap();
            Ok(has_value(inner, index as i32) == 1)
        }
    }

    pub fn max_len(&self) -> Result<usize, V8ArrayError> {
        let inner = self.0.get_weak_with_validation()?;
        unsafe {
            let get_len = (*inner).get_array_length.unwrap();
            Ok(get_len(inner) as usize)
        }
    }

    pub fn get(&self, index: usize) -> Result<V8Value, V8ArrayError> {
        let inner = self.0.get_weak_with_validation()?;
        unsafe {
            let get_value = (*inner).get_value_byindex.unwrap();
            let value = get_value(inner, index as i32);
            if value.is_null() {
                return Err(V8ArrayError::ElementNotFound(index));
            }

            Ok(V8Value::from_raw(value))
        }
    }

    pub fn set(&mut self, index: usize, value: &V8Value) -> Result<(), V8ArrayError> {
        let inner = self.0.get_weak_with_validation()?;
        unsafe {
            let set_value = (*inner).set_value_byindex.unwrap();
            let value = value.get_raw()?;
            if set_value(inner, index as i32, value) != 1 {
                return Err(V8ArrayError::SetFailed(index));
            }

            Ok(())
        }
    }

    pub fn delete(&mut self, index: usize) -> Result<(), V8ArrayError> {
        let inner = self.0.get_weak_with_validation()?;
        unsafe {
            let delete_value = (*inner).delete_value_byindex.unwrap();
            if delete_value(inner, index as i32) != 1 {
                return Err(V8ArrayError::DeleteFailed(index));
            }
            Ok(())
        }
    }
}

impl From<V8Array> for V8Value {
    fn from(value: V8Array) -> Self {
        Self::Array(value)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum V8ArrayError {
    #[error("V8Array is no longer valid.")]
    ArrayNotValid(#[from] ValidatedError),

    #[error(transparent)]
    V8ValueError(#[from] V8ValueError),

    #[error("V8Array element not found at index \"{0}\".")]
    ElementNotFound(usize),

    #[error("Failed to set V8Array at index \"{0}\".")]
    SetFailed(usize),

    #[error("Failed to delete V8Array element at index \"{0}\".")]
    DeleteFailed(usize),
}
