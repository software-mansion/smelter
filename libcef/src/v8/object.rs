use crate::{cef_ref::CefRc, cef_string::CefString, validated::ValidatedError};
use log::error;

use super::{
    value::{V8Value, V8ValueError},
    V8ContextEntered, V8FunctionError,
};

mod document;
mod dom_rect;
mod element;
mod global;

pub use document::*;
pub use dom_rect::*;
pub use element::*;
pub use global::*;

pub struct V8Object(pub(super) CefRc<libcef_sys::cef_v8value_t>);

impl V8Object {
    pub fn has(&self, key: &str) -> Result<bool, V8ObjectError> {
        let inner = self.0.get_weak_with_validation()?;
        let key = CefString::new(key);
        unsafe {
            let has_value = (*inner).has_value_bykey.unwrap();
            Ok(has_value(inner, key.raw()) == 1)
        }
    }

    pub fn get(&self, key: &str) -> Result<V8Value, V8ObjectError> {
        let inner = self.0.get_weak_with_validation()?;
        let cef_key = CefString::new(key);
        unsafe {
            let get_value = (*inner).get_value_bykey.unwrap();
            let value = get_value(inner, cef_key.raw());
            if value.is_null() {
                return Err(V8ObjectError::FieldNotFound(key.to_string()));
            }

            Ok(V8Value::from_raw(value))
        }
    }

    pub fn set(
        &mut self,
        key: &str,
        value: &V8Value,
        attribute: V8PropertyAttribute,
        _context_entered: &V8ContextEntered,
    ) -> Result<(), V8ObjectError> {
        let inner = self.0.get_weak_with_validation()?;
        let cef_key = CefString::new(key);
        unsafe {
            let set_value = (*inner).set_value_bykey.unwrap();
            let value = value.get_raw()?;
            if set_value(inner, cef_key.raw(), value, attribute as u32) != 1 {
                return Err(V8ObjectError::SetFailed(key.to_string()));
            }
            Ok(())
        }
    }

    pub fn delete(
        &mut self,
        key: &str,
        _context_entered: &V8ContextEntered,
    ) -> Result<(), V8ObjectError> {
        let inner = self.0.get_weak_with_validation()?;
        let cef_key = CefString::new(key);
        unsafe {
            let delete_value = (*inner).delete_value_bykey.unwrap();
            if delete_value(inner, cef_key.raw()) != 1 {
                return Err(V8ObjectError::DeleteFailed(key.to_string()));
            }

            Ok(())
        }
    }

    pub fn call_method(
        &self,
        name: &str,
        args: &[&V8Value],
        ctx_entered: &V8ContextEntered,
    ) -> Result<V8Value, V8ObjectError> {
        let V8Value::Function(method) = self.get(name)? else {
            return Err(V8ObjectError::ExpectedType {
                name: name.to_owned(),
                expected: "method".to_owned(),
            });
        };

        method
            .call_as_method(self, args, ctx_entered)
            .map_err(|err| V8ObjectError::MethodCallFailed(err, name.to_string()))
    }

    pub fn get_number(&self, key: &str) -> Result<f64, V8ObjectError> {
        let value = match self.get(key)? {
            V8Value::Double(v) => v.get()?,
            V8Value::Int(v) => v.get()? as f64,
            V8Value::Uint(v) => v.get()? as f64,
            _ => {
                return Err(V8ObjectError::ExpectedType {
                    name: key.to_owned(),
                    expected: "number".to_owned(),
                })
            }
        };

        Ok(value)
    }
}

#[repr(u32)]
pub enum V8PropertyAttribute {
    None = libcef_sys::cef_v8_propertyattribute_t_V8_PROPERTY_ATTRIBUTE_NONE,
    ReadOnly = libcef_sys::cef_v8_propertyattribute_t_V8_PROPERTY_ATTRIBUTE_READONLY,
    DoNotEnum = libcef_sys::cef_v8_propertyattribute_t_V8_PROPERTY_ATTRIBUTE_DONTENUM,
    DoNotDelete = libcef_sys::cef_v8_propertyattribute_t_V8_PROPERTY_ATTRIBUTE_DONTDELETE,
}

#[derive(Debug, thiserror::Error)]
pub enum V8ObjectError {
    #[error("V8Object is no longer valid.")]
    ObjectNotValid(#[from] ValidatedError),

    #[error(transparent)]
    V8ValueError(#[from] V8ValueError),

    #[error("\"{0}\" field not found.")]
    FieldNotFound(String),

    #[error("Failed to set \"{0}\" field.")]
    SetFailed(String),

    #[error("Failed to delete \"{0}\" field.")]
    DeleteFailed(String),

    #[error("Expected \"{name}\" to be a {expected}.")]
    ExpectedType { name: String, expected: String },

    #[error("Failed to call \"{1}\" method.")]
    MethodCallFailed(#[source] V8FunctionError, String),
}
