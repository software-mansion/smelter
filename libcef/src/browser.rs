use std::os::raw::c_int;

use crate::{
    cef_ref::{CefRc, CefRefCountable},
    cef_string::CefString,
    frame::Frame,
    validated::{Validatable, ValidatedError},
};

/// Wrapper over raw [`libcef_sys::cef_browser_t`].
/// Used for interacting with a browser
pub struct Browser {
    inner: CefRc<libcef_sys::cef_browser_t>,
}

impl Browser {
    pub(crate) fn new(browser: *mut libcef_sys::cef_browser_t) -> Self {
        let inner = CefRc::new(browser);
        Self { inner }
    }

    pub fn is_loading(&self) -> Result<bool, BrowserError> {
        unsafe {
            let browser = self.inner.get_weak_with_validation()?;
            let is_loading = (*browser).is_loading.unwrap();
            Ok(is_loading(browser) == 1)
        }
    }

    pub fn main_frame(&self) -> Result<Frame, BrowserError> {
        unsafe {
            let browser = self.inner.get_weak_with_validation()?;
            let get_main_frame = (*browser).get_main_frame.unwrap();
            Ok(Frame::new(get_main_frame(browser)))
        }
    }

    pub fn close(&mut self) -> Result<(), BrowserError> {
        unsafe {
            let browser = self.inner.get_weak_with_validation()?;
            let get_host = (*browser).get_host.unwrap();
            let host = get_host(browser);
            let close_browser = (*host).close_browser.unwrap();
            // `true` means that the close request won't be cancelable
            close_browser(host, true as c_int);
        }

        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum BrowserError {
    #[error("Browser is not alive")]
    NotAlive(#[from] ValidatedError),
}

impl Validatable for libcef_sys::cef_browser_t {
    fn is_valid(&mut self) -> bool {
        match self.is_valid {
            Some(is_valid) => unsafe { is_valid(self) == 1 },
            None => false,
        }
    }
}

impl CefRefCountable for libcef_sys::cef_browser_t {
    fn base_mut(&mut self) -> *mut libcef_sys::cef_base_ref_counted_t {
        &mut self.base
    }
}

#[derive(Default)]
pub struct BrowserSettings {
    pub windowless_frame_rate: i32,
    pub background_color: u32,
}

impl BrowserSettings {
    pub(crate) fn into_raw(self) -> libcef_sys::cef_browser_settings_t {
        libcef_sys::_cef_browser_settings_t {
            size: std::mem::size_of::<libcef_sys::cef_browser_settings_t>(),
            windowless_frame_rate: self.windowless_frame_rate,
            standard_font_family: CefString::empty_raw(),
            fixed_font_family: CefString::empty_raw(),
            serif_font_family: CefString::empty_raw(),
            sans_serif_font_family: CefString::empty_raw(),
            cursive_font_family: CefString::empty_raw(),
            fantasy_font_family: CefString::empty_raw(),
            default_font_size: 0,
            default_fixed_font_size: 0,
            minimum_font_size: 0,
            minimum_logical_font_size: 0,
            default_encoding: CefString::empty_raw(),
            remote_fonts: libcef_sys::cef_state_t_STATE_DEFAULT,
            javascript: libcef_sys::cef_state_t_STATE_DEFAULT,
            javascript_close_windows: libcef_sys::cef_state_t_STATE_DEFAULT,
            javascript_access_clipboard: libcef_sys::cef_state_t_STATE_DEFAULT,
            javascript_dom_paste: libcef_sys::cef_state_t_STATE_DEFAULT,
            image_loading: libcef_sys::cef_state_t_STATE_DEFAULT,
            image_shrink_standalone_to_fit: libcef_sys::cef_state_t_STATE_DEFAULT,
            text_area_resize: libcef_sys::cef_state_t_STATE_DEFAULT,
            tab_to_links: libcef_sys::cef_state_t_STATE_DEFAULT,
            local_storage: libcef_sys::cef_state_t_STATE_DEFAULT,
            databases_deprecated: libcef_sys::cef_state_t_STATE_DEFAULT,
            webgl: libcef_sys::cef_state_t_STATE_DEFAULT,
            background_color: self.background_color,
            chrome_zoom_bubble: libcef_sys::cef_state_t_STATE_DISABLED,
            chrome_status_bubble: libcef_sys::cef_state_t_STATE_DEFAULT,
        }
    }
}
