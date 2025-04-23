use crate::{
    cef::{ProcessId, ProcessMessage, ThreadId, V8Context},
    cef_ref::{CefRc, CefRefCountable},
    validated::{Validatable, ValidatedError},
};

/// Represents a renderable surface.
/// Each browser has a main frame which is the visible web page.
/// Browser can also have multiple smaller frames (for example when `<iframe>` is used)
pub struct Frame {
    inner: CefRc<chromium_sys::cef_frame_t>,
}

impl Frame {
    pub(crate) fn new(frame: *mut chromium_sys::cef_frame_t) -> Self {
        let inner = CefRc::new(frame);
        Self { inner }
    }

    /// Sends IPC message
    pub fn send_process_message(
        &self,
        pid: ProcessId,
        msg: ProcessMessage,
    ) -> Result<(), FrameError> {
        unsafe {
            let frame = self.inner.get_weak_with_validation()?;
            let send_message = (*frame).send_process_message.unwrap();
            send_message(frame, pid as u32, msg.inner.get());
        }

        Ok(())
    }

    /// If called on the renderer process it returns `Ok(V8Context)`, otherwise it's `Err(FrameError::V8ContextWrongThread)`
    pub fn v8_context(&self) -> Result<V8Context, FrameError> {
        let frame = self.inner.get_weak_with_validation()?;

        unsafe {
            if chromium_sys::cef_currently_on(ThreadId::Renderer as u32) != 1 {
                return Err(FrameError::V8ContextWrongThread);
            }

            let get_v8_context = (*frame).get_v8context.unwrap();
            let context = get_v8_context(frame);
            Ok(V8Context::new(context))
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum FrameError {
    #[error("Frame is not longer valid")]
    NotValid(#[from] ValidatedError),

    #[error("Tried to retrieve V8Context on a wrong thread")]
    V8ContextWrongThread,
}

impl Validatable for chromium_sys::cef_frame_t {
    fn is_valid(&mut self) -> bool {
        match self.is_valid {
            Some(is_valid) => unsafe { is_valid(self) == 1 },
            None => false,
        }
    }
}

impl CefRefCountable for chromium_sys::cef_frame_t {
    fn base_mut(&mut self) -> *mut chromium_sys::cef_base_ref_counted_t {
        &mut self.base
    }
}
