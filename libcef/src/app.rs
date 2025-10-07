use crate::{
    RenderProcessHandler, RenderProcessHandlerWrapper,
    cef_ref::{CefRc, CefRefData, CefStruct},
    cef_string::CefString,
    command_line::CommandLine,
};

/// [`App`] is used during process initialization.
/// Configures the way process behaves.
pub trait App {
    type RenderProcessHandlerType: RenderProcessHandler;

    /// Allows setting command line arguments which are passed to CEF later on.
    /// It's called before process is initalized. If called on the main thread, `process_type` is empty.
    /// [List of possible command line arguments](https://peter.sh/experiments/chromium-command-line-switches/)
    fn on_before_command_line_processing(
        &mut self,
        _process_type: String,
        _command_line: &mut CommandLine,
    ) {
    }

    /// Used for specifying renderer process handler.
    /// Called by Chromium every time a new IPC message is received by a renderer process.
    fn render_process_handler(&self) -> Option<Self::RenderProcessHandlerType> {
        None
    }
}

pub(crate) struct AppWrapper<A: App> {
    app: A,
    render_process_handler: Option<CefRc<libcef_sys::cef_render_process_handler_t>>,
}

impl<A: App> CefStruct for AppWrapper<A> {
    type CefType = libcef_sys::cef_app_t;

    fn new_cef_data() -> Self::CefType {
        libcef_sys::cef_app_t {
            base: unsafe { std::mem::zeroed() },
            on_before_command_line_processing: Some(Self::on_before_command_line_processing),
            on_register_custom_schemes: None,
            get_resource_bundle_handler: None,
            get_browser_process_handler: None,
            get_render_process_handler: Some(Self::render_process_handler),
        }
    }

    fn base_from_cef_data(cef_data: &mut Self::CefType) -> &mut libcef_sys::cef_base_ref_counted_t {
        &mut cef_data.base
    }
}

impl<A: App> AppWrapper<A> {
    pub(crate) fn new(app: A) -> Self {
        let render_process_handler = app
            .render_process_handler()
            .map(RenderProcessHandlerWrapper)
            .map(CefRefData::new_ptr)
            .map(CefRc::new);

        Self {
            app,
            render_process_handler,
        }
    }

    extern "C" fn on_before_command_line_processing(
        self_: *mut libcef_sys::cef_app_t,
        process_type: *const libcef_sys::cef_string_t,
        command_line: *mut libcef_sys::cef_command_line_t,
    ) {
        let self_ref = unsafe { CefRefData::<Self>::from_cef(self_) };
        let mut command_line = CommandLine(command_line);
        let process_type = CefString::from_raw(process_type);
        self_ref
            .app
            .on_before_command_line_processing(process_type, &mut command_line);
    }

    extern "C" fn render_process_handler(
        self_: *mut libcef_sys::cef_app_t,
    ) -> *mut libcef_sys::cef_render_process_handler_t {
        let self_ref = unsafe { CefRefData::<Self>::from_cef(self_) };
        match self_ref.render_process_handler {
            Some(ref handler) => handler.get(),
            None => std::ptr::null_mut(),
        }
    }
}
