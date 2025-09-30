use crate::cef::{V8Document, V8Object, V8ObjectError, V8Value};
use crate::v8::{V8ContextEntered, V8PropertyAttribute};

pub struct V8Global(pub V8Object);

impl V8Global {
    pub fn has(&self, key: &str) -> Result<bool, V8ObjectError> {
        self.0.has(key)
    }

    pub fn get(&self, key: &str) -> Result<V8Value, V8ObjectError> {
        self.0.get(key)
    }

    pub fn set(
        &mut self,
        key: &str,
        value: &V8Value,
        attribute: V8PropertyAttribute,
        ctx_entered: &V8ContextEntered,
    ) -> Result<(), V8ObjectError> {
        self.0.set(key, value, attribute, ctx_entered)
    }

    pub fn delete(
        &mut self,
        key: &str,
        ctx_entered: &V8ContextEntered,
    ) -> Result<(), V8ObjectError> {
        self.0.delete(key, ctx_entered)
    }

    pub fn call_method(
        &self,
        name: &str,
        args: &[&V8Value],
        ctx_entered: &V8ContextEntered,
    ) -> Result<V8Value, V8ObjectError> {
        self.0.call_method(name, args, ctx_entered)
    }

    pub fn document(&self) -> Result<V8Document, V8ObjectError> {
        let V8Value::Object(document) = self.0.get("document")? else {
            return Err(V8ObjectError::ExpectedType {
                name: "document".to_owned(),
                expected: "document object".to_owned(),
            });
        };

        Ok(V8Document(document))
    }
}
