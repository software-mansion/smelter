use std::os::raw::c_int;

use crate::cef_string::CefString;

pub struct WindowInfo {
    pub windowless_rendering_enabled: bool,
}

impl WindowInfo {
    #[cfg(target_os = "macos")]
    pub(crate) fn into_raw(self) -> chromium_sys::cef_window_info_t {
        chromium_sys::cef_window_info_t {
            window_name: CefString::empty_raw(),
            bounds: unsafe { std::mem::zeroed() },
            hidden: true as c_int,
            parent_view: std::ptr::null_mut(),
            windowless_rendering_enabled: self.windowless_rendering_enabled as c_int,
            shared_texture_enabled: false as c_int,
            external_begin_frame_enabled: false as c_int,
            view: std::ptr::null_mut(),
            runtime_style: chromium_sys::cef_runtime_style_t_CEF_RUNTIME_STYLE_ALLOY,
        }
    }

    #[cfg(target_os = "linux")]
    pub(crate) fn into_raw(self) -> chromium_sys::cef_window_info_t {
        use std::os::raw::c_ulong;

        chromium_sys::cef_window_info_t {
            window_name: CefString::empty_raw(),
            bounds: unsafe { std::mem::zeroed() },
            parent_window: 0 as c_ulong,
            windowless_rendering_enabled: self.windowless_rendering_enabled as c_int,
            shared_texture_enabled: false as c_int,
            external_begin_frame_enabled: false as c_int,
            window: 0 as c_ulong,
            runtime_style: chromium_sys::cef_runtime_style_t_CEF_RUNTIME_STYLE_ALLOY,
        }
    }
}
