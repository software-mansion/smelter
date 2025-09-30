use std::os::raw::{c_int, c_void};

use crate::{
    browser::Browser,
    cef_ref::{CefRefCountable, CefRefData, CefStruct},
};

pub struct Resolution {
    pub width: usize,
    pub height: usize,
}

/// Handles browser render callbacks
pub trait RenderHandler {
    /// Specifies the render resolution
    fn resolution(&self, browser: &Browser) -> Resolution;

    /// Called every time new frame is rendered
    fn on_paint(&self, browser: &Browser, buffer: &[u8], resolution: Resolution);
}

pub(crate) struct RenderHandlerWrapper<R: RenderHandler>(pub R);

impl<R: RenderHandler> CefStruct for RenderHandlerWrapper<R> {
    type CefType = chromium_sys::cef_render_handler_t;

    fn new_cef_data() -> Self::CefType {
        chromium_sys::cef_render_handler_t {
            base: unsafe { std::mem::zeroed() },
            get_accessibility_handler: None,
            get_root_screen_rect: None,
            get_view_rect: Some(Self::view_rect),
            get_screen_point: None,
            get_screen_info: None,
            on_popup_show: None,
            on_popup_size: None,
            on_paint: Some(Self::on_paint),
            on_accelerated_paint: None,
            get_touch_handle_size: None,
            on_touch_handle_state_changed: None,
            start_dragging: None,
            update_drag_cursor: None,
            on_scroll_offset_changed: None,
            on_ime_composition_range_changed: None,
            on_text_selection_changed: None,
            on_virtual_keyboard_requested: None,
        }
    }

    fn base_from_cef_data(
        cef_data: &mut Self::CefType,
    ) -> &mut chromium_sys::cef_base_ref_counted_t {
        &mut cef_data.base
    }
}

impl CefRefCountable for chromium_sys::cef_render_handler_t {
    fn base_mut(&mut self) -> *mut chromium_sys::cef_base_ref_counted_t {
        &mut self.base
    }
}

impl<R: RenderHandler> RenderHandlerWrapper<R> {
    extern "C" fn view_rect(
        self_: *mut chromium_sys::cef_render_handler_t,
        browser: *mut chromium_sys::cef_browser_t,
        rect: *mut chromium_sys::cef_rect_t,
    ) {
        unsafe {
            let self_ref = CefRefData::<Self>::from_cef(self_);
            let browser = Browser::new(browser);
            let resolution = self_ref.0.resolution(&browser);
            let rect = &mut *rect;
            rect.width = resolution.width as i32;
            rect.height = resolution.height as i32;
        }
    }

    extern "C" fn on_paint(
        self_: *mut chromium_sys::cef_render_handler_t,
        browser: *mut chromium_sys::cef_browser_t,
        _type: chromium_sys::cef_paint_element_type_t,
        _dirty_rects_count: usize,
        _dirt_rects: *const chromium_sys::cef_rect_t,
        buffer: *const c_void,
        width: c_int,
        height: c_int,
    ) {
        unsafe {
            let self_ref = CefRefData::<Self>::from_cef(self_);
            let browser = Browser::new(browser);
            let buffer =
                std::slice::from_raw_parts(buffer as *const u8, (4 * width * height) as usize);
            self_ref.0.on_paint(
                &browser,
                buffer,
                Resolution {
                    width: width as usize,
                    height: height as usize,
                },
            );
        }
    }
}
