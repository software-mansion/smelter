use widestring::U16CString;

/// Helper for handling UTF-16 `cef_string_t`
pub struct CefString(libcef_sys::cef_string_t);

impl Drop for CefString {
    fn drop(&mut self) {
        unsafe {
            let _ = U16CString::from_raw(self.0.str_);
        }
    }
}

impl CefString {
    /// The returned string is owned by the caller and automatically freed on drop.
    ///
    /// # Safety
    /// Make sure that CEF does not free it.
    pub fn new<S: Into<String>>(s: S) -> Self {
        let str_value = U16CString::from_str(s.into()).unwrap();
        Self(libcef_sys::cef_string_utf16_t {
            length: str_value.len(),
            str_: str_value.into_raw(),
            dtor: None,
        })
    }

    /// The returned string is not automatically freed on drop. It's caller's or CEF's responsibility to
    /// free it.
    pub fn new_raw<S: Into<String>>(s: S) -> libcef_sys::cef_string_t {
        extern "C" fn dtor(ptr: *mut u16) {
            if !ptr.is_null() {
                unsafe {
                    let _ = U16CString::from_raw(ptr);
                }
            }
        }
        let str_value: String = s.into();
        let raw_value = U16CString::from_str(&str_value).unwrap().into_raw();
        libcef_sys::cef_string_utf16_t {
            length: str_value.len(),
            str_: raw_value,
            dtor: Some(dtor),
        }
    }

    pub fn raw(&self) -> &libcef_sys::cef_string_t {
        &self.0
    }

    /// Returns Rust's `String` from UTF-16 `cef_string_t`.
    /// If `ptr` is null, empty string is returned
    pub fn from_raw(ptr: *const libcef_sys::cef_string_t) -> String {
        if ptr.is_null() {
            return String::new();
        }

        unsafe {
            let cef_str = *ptr;
            U16CString::from_ptr(cef_str.str_, cef_str.length)
                .unwrap()
                .to_string_lossy()
        }
    }

    pub fn from_userfree(ptr: libcef_sys::cef_string_userfree_utf16_t) -> String {
        let cef_string = CefString::from_raw(ptr);
        if !ptr.is_null() {
            unsafe {
                // `CefString:from_raw` creates a string copy so it's safe to free the memory
                libcef_sys::cef_string_userfree_utf16_free(ptr);
            }
        }

        cef_string
    }

    /// Creates a new empty `cef_string_t`
    pub fn empty_raw() -> libcef_sys::cef_string_t {
        unsafe { std::mem::zeroed() }
    }
}
