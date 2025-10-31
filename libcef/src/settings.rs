use std::{env, os::raw::c_int, path::PathBuf};

use libcef_sys::_cef_string_utf16_t;

use crate::cef_string::CefString;

pub const PROCESS_HELPER_PATH_ENV: &str = "SMELTER_PROCESS_HELPER_PATH";

/// Main process settings
#[derive(Default)]
pub struct Settings {
    /// Path to root directory of CEF cache. The path should be unique for each running instance.
    pub root_cache_path: PathBuf,
    /// If set to `true` message loop can run on a separate thread. **Not supported by MacOS**
    pub multi_threaded_message_loop: bool,
    /// If set to `true` it makes it possible to control message pump scheduling.
    /// Useful in combination with [`Context::do_message_loop_work`](crate::context::Context)
    pub external_message_pump: bool,
    pub windowless_rendering_enabled: bool,
    pub log_severity: LogSeverity,
    pub remote_debugging_port: u16,
    pub background_color: u32,
}

impl Settings {
    pub fn into_raw(self) -> libcef_sys::cef_settings_t {
        let (main_path, helper_path) = executables_paths();

        libcef_sys::cef_settings_t {
            size: std::mem::size_of::<libcef_sys::cef_settings_t>(),
            no_sandbox: true as c_int,
            browser_subprocess_path: helper_path,
            framework_dir_path: CefString::empty_raw(),
            main_bundle_path: main_path,
            chrome_policy_id: CefString::empty_raw(),
            chrome_app_icon_id: 0,
            disable_signal_handlers: false as c_int,
            multi_threaded_message_loop: self.multi_threaded_message_loop as c_int,
            external_message_pump: self.external_message_pump as c_int,
            windowless_rendering_enabled: self.windowless_rendering_enabled as c_int,
            command_line_args_disabled: false as c_int,
            cache_path: CefString::empty_raw(),
            root_cache_path: CefString::new_raw(self.root_cache_path.display().to_string()),
            persist_session_cookies: false as c_int,
            user_agent: CefString::empty_raw(),
            user_agent_product: CefString::empty_raw(),
            locale: CefString::empty_raw(),
            log_file: CefString::empty_raw(),
            log_severity: self.log_severity as u32,
            log_items: libcef_sys::cef_log_items_t_LOG_ITEMS_DEFAULT,
            javascript_flags: CefString::empty_raw(),
            resources_dir_path: CefString::empty_raw(),
            locales_dir_path: CefString::empty_raw(),
            remote_debugging_port: self.remote_debugging_port as c_int,
            uncaught_exception_stack_size: 0 as c_int,
            background_color: self.background_color,
            accept_language_list: CefString::empty_raw(),
            cookieable_schemes_list: CefString::empty_raw(),
            cookieable_schemes_exclude_defaults: false as c_int,
        }
    }
}

#[repr(u32)]
#[derive(Default)]
pub enum LogSeverity {
    #[default]
    Default = libcef_sys::cef_log_severity_t_LOGSEVERITY_DEFAULT,
    Debug = libcef_sys::cef_log_severity_t_LOGSEVERITY_DEBUG,
    Info = libcef_sys::cef_log_severity_t_LOGSEVERITY_INFO,
    Warning = libcef_sys::cef_log_severity_t_LOGSEVERITY_WARNING,
    Error = libcef_sys::cef_log_severity_t_LOGSEVERITY_ERROR,
    Fatal = libcef_sys::cef_log_severity_t_LOGSEVERITY_FATAL,
    Disable = libcef_sys::cef_log_severity_t_LOGSEVERITY_DISABLE,
}


#[cfg(target_os = "linux")]
fn executables_paths() -> (_cef_string_utf16_t, _cef_string_utf16_t) {
    let browser_subprocess_path = env::var(PROCESS_HELPER_PATH_ENV).unwrap_or_else(|_| {
        let current_exe = env::current_exe().unwrap();
        let current_dir = current_exe.parent().unwrap();
        current_dir.join("process_helper").display().to_string()
    });

    (
        CefString::empty_raw(),
        CefString::new_raw(browser_subprocess_path),
    )
}

#[cfg(target_os = "macos")]
fn executables_paths() -> (_cef_string_utf16_t, _cef_string_utf16_t) {
    use std::path::PathBuf;

    let current_exe = env::current_exe().unwrap();
    let current_dir = current_exe.parent().unwrap();

    let main_bundle_path = PathBuf::from(current_dir).join("smelter.app");

    let browser_subprocess_path = main_bundle_path
        .join("Contents")
        .join("Frameworks")
        .join("smelter Helper.app")
        .join("Contents")
        .join("MacOS")
        .join("smelter Helper");

    (
        CefString::new_raw(main_bundle_path.display().to_string()),
        CefString::new_raw(browser_subprocess_path.display().to_string()),
    )
}
