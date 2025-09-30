use std::os::raw::c_int;

use crate::{
    browser::Browser,
    cef::{Frame, ProcessId, ProcessMessage},
    cef_ref::{CefRc, CefRefData, CefStruct},
    render_handler::{RenderHandler, RenderHandlerWrapper},
};

/// [`Client`] is used for setting browser-specific handlers.
/// It should be defined for each browser session.
pub trait Client {
    type RenderHandlerType: RenderHandler;

    /// Returns browser's render handler.
    /// **Important:** it's called every time a frame is rendered
    fn render_handler(&self) -> Option<Self::RenderHandlerType> {
        None
    }

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

pub(crate) struct ClientWrapper<C: Client> {
    client: C,
    render_handler: Option<CefRc<libcef_sys::cef_render_handler_t>>,
}

impl<C: Client> CefStruct for ClientWrapper<C> {
    type CefType = libcef_sys::cef_client_t;

    fn new_cef_data() -> Self::CefType {
        libcef_sys::cef_client_t {
            base: unsafe { std::mem::zeroed() },
            get_audio_handler: None,
            get_command_handler: None,
            get_context_menu_handler: None,
            get_dialog_handler: None,
            get_display_handler: None,
            get_download_handler: None,
            get_drag_handler: None,
            get_find_handler: None,
            get_focus_handler: None,
            get_frame_handler: None,
            get_permission_handler: None,
            get_jsdialog_handler: None,
            get_keyboard_handler: None,
            get_life_span_handler: None,
            get_load_handler: None,
            get_print_handler: None,
            get_render_handler: Some(Self::render_handler),
            get_request_handler: None,
            on_process_message_received: Some(Self::on_process_message_received),
        }
    }

    fn base_from_cef_data(cef_data: &mut Self::CefType) -> &mut libcef_sys::cef_base_ref_counted_t {
        &mut cef_data.base
    }
}

impl<C: Client> ClientWrapper<C> {
    pub fn new(client: C) -> Self {
        let render_handler = client
            .render_handler()
            .map(RenderHandlerWrapper)
            .map(CefRefData::new_ptr)
            .map(CefRc::new);
        Self {
            client,
            render_handler,
        }
    }

    extern "C" fn render_handler(
        self_: *mut libcef_sys::cef_client_t,
    ) -> *mut libcef_sys::cef_render_handler_t {
        unsafe {
            let self_ref = CefRefData::<Self>::from_cef(self_);
            match &self_ref.render_handler {
                Some(handler) => handler.get(),
                None => std::ptr::null_mut(),
            }
        }
    }

    extern "C" fn on_process_message_received(
        self_: *mut libcef_sys::cef_client_t,
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

        let is_handled = self_ref.client.on_process_message_received(
            &browser,
            &frame,
            source_process.into(),
            &message,
        );

        is_handled as c_int
    }
}
