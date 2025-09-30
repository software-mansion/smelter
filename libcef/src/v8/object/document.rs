use crate::cef::{V8ContextEntered, V8Element, V8Object, V8ObjectError, V8String, V8Value};

pub struct V8Document(pub V8Object);

impl V8Document {
    pub fn element_by_id(
        &self,
        id: &str,
        ctx_entered: &V8ContextEntered,
    ) -> Result<V8Element, V8ObjectError> {
        let id_value: V8Value = V8String::new(id).into();
        let element = self
            .0
            .call_method("getElementById", &[&id_value], ctx_entered)?;
        let V8Value::Object(element) = element else {
            return Err(V8ObjectError::ExpectedType {
                name: format!("getElementById({id})"),
                expected: "element object".to_owned(),
            });
        };

        Ok(V8Element(element))
    }
}
