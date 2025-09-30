use std::os::raw::c_int;

use crate::{
    cef::{Browser, Frame, ProcessId, ProcessMessage, V8Context},
    cef_ref::{CefRc, CefRefCountable, CefRefData, CefStruct},
};

/// Handles renderer process callbacks.
/// Methods are only called by CEF when implemented on the renderer process
pub trait RenderProcessHandler {
    /// Called when a new [`V8Context`] for a [`Frame`] was created
    fn on_context_created(&mut self, _browser: &Browser, _frame: &Frame, _context: &V8Context) {}

    /// Called when new process message is received.
    /// Return `true` if message was handled, `false` otherwise
    fn on_process_message_received(
        &mut self,
        _browser: &Browser,
        _frame: &Frame,
        _source_process: ProcessId,
        _message: &ProcessMessage,
    ) -> bool {
        false
    }
}

impl RenderProcessHandler for () {}

pub(crate) struct RenderProcessHandlerWrapper<R: RenderProcessHandler>(pub R);

impl<R: RenderProcessHandler> CefStruct for RenderProcessHandlerWrapper<R> {
    type CefType = libcef_sys::cef_render_process_handler_t;

    fn new_cef_data() -> Self::CefType {
        libcef_sys::cef_render_process_handler_t {
            base: unsafe { std::mem::zeroed() },
            on_web_kit_initialized: None,
            on_browser_created: None,
            on_browser_destroyed: None,
            get_load_handler: None,
            on_context_created: Some(Self::on_context_created),
            on_context_released: None,
            on_uncaught_exception: None,
            on_focused_node_changed: None,
            on_process_message_received: Some(Self::on_process_message_received),
        }
    }

    fn base_from_cef_data(cef_data: &mut Self::CefType) -> &mut libcef_sys::cef_base_ref_counted_t {
        &mut cef_data.base
    }
}

impl<R: RenderProcessHandler> RenderProcessHandlerWrapper<R> {
    extern "C" fn on_context_created(
        self_: *mut libcef_sys::cef_render_process_handler_t,
        browser: *mut libcef_sys::cef_browser_t,
        frame: *mut libcef_sys::cef_frame_t,
        context: *mut libcef_sys::cef_v8context_t,
    ) {
        let self_ref = unsafe { CefRefData::<Self>::from_cef(self_) };
        let browser = Browser::new(browser);
        let frame = Frame::new(frame);
        let v8_context = V8Context::new(context);
        self_ref.0.on_context_created(&browser, &frame, &v8_context);
    }

    extern "C" fn on_process_message_received(
        self_: *mut libcef_sys::cef_render_process_handler_t,
        browser: *mut libcef_sys::cef_browser_t,
        frame: *mut libcef_sys::cef_frame_t,
        source_process: libcef_sys::cef_process_id_t,
        message: *mut libcef_sys::cef_process_message_t,
    ) -> c_int {
        let self_ref = unsafe { CefRefData::<Self>::from_cef(self_) };

        let browser = Browser::new(browser);
        let frame = Frame::new(frame);
        let message = ProcessMessage {
            inner: CefRc::new(message),
        };

        let is_handled = self_ref.0.on_process_message_received(
            &browser,
            &frame,
            source_process.into(),
            &message,
        );

        is_handled as c_int
    }
}

impl CefRefCountable for libcef_sys::cef_render_process_handler_t {
    fn base_mut(&mut self) -> *mut libcef_sys::cef_base_ref_counted_t {
        &mut self.base
    }
}
