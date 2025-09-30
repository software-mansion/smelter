use libcef_sys::_cef_task_t;

use crate::cef_ref::{CefRefData, CefStruct};

/// Runs functions on a specified thread
pub struct Task<F: FnOnce()> {
    inner: Option<F>,
}

impl<F: FnOnce()> CefStruct for Task<F> {
    type CefType = libcef_sys::cef_task_t;

    fn new_cef_data() -> Self::CefType {
        libcef_sys::cef_task_t {
            base: unsafe { std::mem::zeroed() },
            execute: Some(Self::execute),
        }
    }

    fn base_from_cef_data(cef_data: &mut Self::CefType) -> &mut libcef_sys::cef_base_ref_counted_t {
        &mut cef_data.base
    }
}

impl<F: FnOnce()> Task<F> {
    pub fn new(task: F) -> Self {
        Self { inner: Some(task) }
    }

    pub fn run(mut self, thread_id: ThreadId) {
        unsafe {
            if libcef_sys::cef_currently_on(thread_id as u32) == 1 {
                if let Some(task) = self.inner.take() {
                    task();
                }
                return;
            }

            let task = CefRefData::new_ptr(self);
            libcef_sys::cef_post_task(thread_id as u32, task);
        }
    }

    extern "C" fn execute(self_: *mut _cef_task_t) {
        let self_ref = unsafe { CefRefData::<Self>::from_cef(self_) };
        if let Some(task) = self_ref.inner.take() {
            task();
        }
    }
}

#[repr(u32)]
#[derive(Debug, Clone, Copy)]
pub enum ThreadId {
    UI = libcef_sys::cef_thread_id_t_TID_UI,
    FileBackground = libcef_sys::cef_thread_id_t_TID_FILE_BACKGROUND,
    FileUserVisible = libcef_sys::cef_thread_id_t_TID_FILE_USER_VISIBLE,
    FileUserBlocking = libcef_sys::cef_thread_id_t_TID_FILE_USER_BLOCKING,
    ProcessLauncher = libcef_sys::cef_thread_id_t_TID_PROCESS_LAUNCHER,
    IO = libcef_sys::cef_thread_id_t_TID_IO,
    Renderer = libcef_sys::cef_thread_id_t_TID_RENDERER,
}
