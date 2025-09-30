use std::io;

use crate::{cef::*, cef_ref::CefRefData, cef_string::CefString, main_args::MainArgs};

/// Handles CEF initialization and deinitialization.
/// Used for interacting with CEF functions
pub struct Context {
    // We allow creating context only with `Context::new` and `Context::new_helper`.
    // This dissallows creating `Context` manually
    _priv: (),
}

impl Drop for Context {
    fn drop(&mut self) {
        unsafe {
            libcef_sys::cef_shutdown();

            #[cfg(target_os = "macos")]
            libcef_sys::cef_unload_library();
        }
    }
}

impl Context {
    /// Creates new context for the main process
    pub fn new<A: App>(app: A, settings: Settings) -> Result<Self, ContextError> {
        #[cfg(target_os = "macos")]
        {
            let framework_path = std::env::current_exe()?
                .parent()
                .unwrap()
                .join("smelter.app")
                .join("Contents")
                .join("Frameworks")
                .join("Chromium Embedded Framework.framework")
                .join("Chromium Embedded Framework");

            Self::load_framework(framework_path)?;
        }

        let mut main_args = MainArgs::from_program_args();
        let settings = settings.into_raw();
        let app = CefRefData::new_ptr(AppWrapper::new(app));

        let init_result = unsafe {
            libcef_sys::cef_initialize(main_args.raw_mut(), &settings, app, std::ptr::null_mut())
        };

        if init_result != 1 {
            return Err(ContextError::FrameworkInitFailed);
        }

        Ok(Context { _priv: () })
    }

    /// Creates new context for a subprocess
    pub fn new_helper() -> Result<Self, ContextError> {
        #[cfg(target_os = "macos")]
        {
            let framework_path = std::env::current_exe()?
                .parent()
                .unwrap()
                .join("..")
                .join("..")
                .join("..")
                .join("Chromium Embedded Framework.framework")
                .join("Chromium Embedded Framework");

            Self::load_framework(framework_path)?;
        }

        Ok(Context { _priv: () })
    }

    #[cfg(target_os = "macos")]
    fn load_framework(framework_path: std::path::PathBuf) -> Result<(), ContextError> {
        use std::ffi::CString;
        let framework_path = CString::new(framework_path.display().to_string()).unwrap();
        let is_loaded = unsafe { libcef_sys::cef_load_library(framework_path.as_ptr()) };
        if is_loaded != 1 {
            return Err(ContextError::FrameworkNotLoaded);
        }

        Ok(())
    }

    /// Launches subprocess
    pub fn execute_process<A: App>(&self, app: A) -> i32 {
        let mut main_args = MainArgs::from_program_args();
        let app = CefRefData::new_ptr(AppWrapper::new(app));
        unsafe { libcef_sys::cef_execute_process(main_args.raw_mut(), app, std::ptr::null_mut()) }
    }

    pub fn start_browser<C: Client>(
        &self,
        client: C,
        window_info: WindowInfo,
        settings: BrowserSettings,
        url: &str,
    ) -> Result<Browser, ContextError> {
        let client = CefRefData::new_ptr(ClientWrapper::new(client));
        let window_info = window_info.into_raw();
        let settings = settings.into_raw();
        let url = CefString::new_raw(url);
        let browser = unsafe {
            libcef_sys::cef_browser_host_create_browser_sync(
                &window_info,
                client,
                &url,
                &settings,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        if browser.is_null() {
            return Err(ContextError::StartBrowserFailed);
        }

        Ok(Browser::new(browser))
    }

    /// Runs Chromium's message loop. It's a blocking call
    pub fn run_message_loop(&self) {
        unsafe {
            libcef_sys::cef_run_message_loop();
        }
    }

    /// Does currently available message loop work. It has to be called periodically.
    /// Calling it too rarely leads to starvation and calling it too often leads to high CPU usage.
    /// In both cases it causes low performance.
    /// It's recommended to use [`Context::run_message_loop`] instead or use external message pump
    /// which is not implemented as of now.
    pub fn do_message_loop_work(&self) {
        // TODO: Implement external message pump
        unsafe {
            libcef_sys::cef_do_message_loop_work();
        }
    }

    pub fn currently_on_thread(&self, thread_id: ThreadId) -> bool {
        unsafe { libcef_sys::cef_currently_on(thread_id as u32) == 1 }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ContextError {
    #[error("IO operation failed")]
    IOError(#[from] io::Error),

    #[error("Failed to load chromium framework lib")]
    FrameworkNotLoaded,

    #[error("Failed to init chromium framework")]
    FrameworkInitFailed,

    #[error("Failed to start browser session")]
    StartBrowserFailed,
}
