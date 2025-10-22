use crate::{cef_ref::CefRc, cef_string::CefString};

use super::value::{V8Value, V8ValueError};

pub struct V8String(pub(super) CefRc<libcef_sys::cef_v8_value_t>);

impl V8String {
    pub fn new(value: &str) -> Self {
        let value = CefString::new(value);
        // `cef_v8value_create_string` copies the string so it's safe to drop `CefString`
        let value = unsafe { libcef_sys::cef_v8_value_create_string(value.raw()) };

        Self(CefRc::new(value))
    }

    pub fn get(&self) -> Result<String, V8ValueError> {
        let inner = self.0.get_weak_with_validation()?;
        unsafe {
            let get_value = (*inner).get_string_value.unwrap();
            let value = get_value(inner);
            Ok(CefString::from_userfree(value))
        }
    }
}

impl From<V8String> for V8Value {
    fn from(value: V8String) -> Self {
        Self::String(value)
    }
}
