use crate::cef_ref::CefRc;

use super::value::{V8Value, V8ValueError};

pub struct V8Int(pub(super) CefRc<libcef_sys::cef_v8value_t>);

impl V8Int {
    pub fn new(value: i32) -> Self {
        let inner = unsafe { libcef_sys::cef_v8value_create_int(value) };
        Self(CefRc::new(inner))
    }

    pub fn get(&self) -> Result<i32, V8ValueError> {
        let inner = self.0.get_weak_with_validation()?;
        unsafe {
            let get_value = (*inner).get_int_value.unwrap();
            Ok(get_value(inner))
        }
    }
}

impl From<V8Int> for V8Value {
    fn from(value: V8Int) -> Self {
        Self::Int(value)
    }
}

pub struct V8Uint(pub(super) CefRc<libcef_sys::cef_v8value_t>);

impl V8Uint {
    pub fn new(value: u32) -> Self {
        let inner = unsafe { libcef_sys::cef_v8value_create_uint(value) };
        Self(CefRc::new(inner))
    }

    pub fn get(&self) -> Result<u32, V8ValueError> {
        let inner = self.0.get_weak_with_validation()?;
        unsafe {
            let get_value = (*inner).get_uint_value.unwrap();
            Ok(get_value(inner))
        }
    }
}

impl From<V8Uint> for V8Value {
    fn from(value: V8Uint) -> Self {
        Self::Uint(value)
    }
}

pub struct V8Double(pub(super) CefRc<libcef_sys::cef_v8value_t>);

impl V8Double {
    pub fn new(value: f64) -> Self {
        let inner = unsafe { libcef_sys::cef_v8value_create_double(value) };
        Self(CefRc::new(inner))
    }

    pub fn get(&self) -> Result<f64, V8ValueError> {
        let inner = self.0.get_weak_with_validation()?;
        unsafe {
            let get_value = (*inner).get_double_value.unwrap();
            Ok(get_value(inner))
        }
    }
}

impl From<V8Double> for V8Value {
    fn from(value: V8Double) -> Self {
        Self::Double(value)
    }
}
