use std::ffi::CString;

/// Holds the processes's program arguments
pub struct MainArgs {
    inner: chromium_sys::cef_main_args_t,

    // We keep it here so that the data is not dropped before it's used
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    _argv: Vec<*mut u8>,
    #[cfg(not(all(target_os = "linux", target_arch = "aarch64")))]
    _argv: Vec<*mut i8>,
}

impl MainArgs {
    pub fn from_program_args() -> Self {
        let mut argv: Vec<_> = std::env::args()
            .map(|arg| CString::new(arg).unwrap().into_raw())
            .collect();
        let inner = chromium_sys::cef_main_args_t {
            argc: argv.len() as i32,
            argv: argv.as_mut_ptr(),
        };

        Self { inner, _argv: argv }
    }

    pub fn raw_mut(&mut self) -> &mut chromium_sys::cef_main_args_t {
        &mut self.inner
    }
}
