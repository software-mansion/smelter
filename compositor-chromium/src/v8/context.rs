use log::warn;

use crate::cef::V8Global;
use crate::cef::V8Value;
use crate::cef_ref::{CefRc, CefRefCountable};
use crate::cef_string::CefString;
use crate::validated::{Validatable, ValidatedError};

use super::V8Object;

/// JavaScript V8 engine context.
/// Available only on the renderer process
pub struct V8Context {
    inner: CefRc<chromium_sys::cef_v8context_t>,
}

impl V8Context {
    pub(crate) fn new(v8_context: *mut chromium_sys::cef_v8context_t) -> Self {
        let inner = CefRc::new(v8_context);
        Self { inner }
    }

    pub fn enter(&self) -> Result<V8ContextEntered<'_>, V8ContextError> {
        unsafe {
            let ctx = self.inner.get_weak_with_validation()?;
            let enter_context = (*ctx).enter.unwrap();
            enter_context(ctx);
        }

        Ok(V8ContextEntered(self))
    }

    pub fn global(&self) -> Result<V8Global, V8ContextError> {
        unsafe {
            let ctx = self.inner.get_weak_with_validation()?;
            let get_global = (*ctx).get_global.unwrap();
            let global = CefRc::new(get_global(ctx));

            Ok(V8Global(V8Object(global)))
        }
    }

    pub fn eval(&self, code: &str) -> Result<V8Value, V8ContextError> {
        unsafe {
            let ctx = self.inner.get_weak_with_validation()?;
            let eval = (*ctx).eval.unwrap();
            let code = CefString::new_raw(code);
            let mut retval: *mut chromium_sys::cef_v8value_t = std::ptr::null_mut();
            let mut exception: *mut chromium_sys::cef_v8exception_t = std::ptr::null_mut();

            eval(ctx, &code, std::ptr::null(), 0, &mut retval, &mut exception);
            if !exception.is_null() {
                let get_message = (*exception).get_message.unwrap();
                let message = CefString::from_userfree(get_message(exception));
                return Err(V8ContextError::EvalFailed(message));
            }

            Ok(V8Value::from_raw(retval))
        }
    }
}

pub struct V8ContextEntered<'a>(&'a V8Context);

impl Drop for V8ContextEntered<'_> {
    fn drop(&mut self) {
        unsafe {
            match self.0.inner.get_weak_with_validation() {
                Ok(ctx) => {
                    let exit_context = (*ctx).exit.unwrap();
                    exit_context(ctx);
                }
                Err(err) => warn!("Could not exit the context: {err}"),
            }
        }
    }
}

impl Validatable for chromium_sys::cef_v8context_t {
    fn is_valid(&mut self) -> bool {
        match self.is_valid {
            Some(is_valid) => unsafe { is_valid(self) == 1 },
            None => false,
        }
    }
}

impl CefRefCountable for chromium_sys::cef_v8context_t {
    fn base_mut(&mut self) -> *mut chromium_sys::cef_base_ref_counted_t {
        &mut self.base
    }
}

#[derive(Debug, thiserror::Error)]
pub enum V8ContextError {
    #[error("V8Context is no longer valid")]
    NotValid(#[from] ValidatedError),

    #[error("Eval failed: {0}")]
    EvalFailed(String),
}
