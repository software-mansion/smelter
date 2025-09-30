use crate::cef::{DOMRect, V8ContextEntered, V8Object, V8ObjectError, V8Value};

pub struct V8Element(pub V8Object);

impl V8Element {
    pub fn bounding_rect(&self, ctx_entered: &V8ContextEntered) -> Result<DOMRect, V8ObjectError> {
        let rect = self
            .0
            .call_method("getBoundingClientRect", &[], ctx_entered)?;
        let V8Value::Object(rect) = rect else {
            return Err(V8ObjectError::ExpectedType {
                name: "getBoundingClientRect()".to_owned(),
                expected: "DOMRect object".to_owned(),
            });
        };

        Ok(DOMRect {
            x: rect.get_number("x")?,
            y: rect.get_number("y")?,
            width: rect.get_number("width")?,
            height: rect.get_number("height")?,
        })
    }
}
