use std::{fmt::Display, sync::Arc};

use crate::{
    transformations::shader::pipeline::{USER_DEFINED_BUFFER_BINDING, USER_DEFINED_BUFFER_GROUP},
    wgpu::common_pipeline::VERTEX_ENTRYPOINT_NAME,
};

const HEADER_DOCS_URL: &str = "https://smelter.dev/docs/concept/shaders#header";

#[derive(Debug, thiserror::Error)]
pub struct ShaderParseError {
    #[source]
    parse_error: naga::front::wgsl::ParseError,
    source: Arc<str>,
}

impl ShaderParseError {
    pub fn new(parse_error: naga::front::wgsl::ParseError, source: Arc<str>) -> Self {
        Self {
            parse_error,
            source,
        }
    }
}

impl Display for ShaderParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.parse_error.location(&self.source) {
            Some(location) => write!(
                f,
                "Shader parsing error in line {} column {}.",
                location.line_number, location.line_position
            ),
            None => f.write_str("Shader parsing error."),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ShaderValidationError {
    #[error("A global \"{0}\" should be declared in the shader. Make sure to include the compositor header code inside your shader. Learn more: {HEADER_DOCS_URL}.")]
    GlobalNotFound(String),

    #[error("A global variable \"{1}\" has a wrong type. Learn more: {HEADER_DOCS_URL}.")]
    GlobalBadType(#[source] TypeEquivalenceError, String),

    #[error("Could not find a vertex shader entrypoint. Expected \"fn {VERTEX_ENTRYPOINT_NAME}(input: VertexInput)\".")]
    VertexShaderNotFound,

    #[error("Wrong vertex shader argument amount: found {0}, expected 1.")]
    VertexShaderBadArgumentAmount(usize),

    #[error(
        "The input type of the vertex shader has to be named \"VertexInput\" (received: \"{0}\")."
    )]
    VertexShaderBadInputTypeName(String),

    #[error("The vertex shader input has a wrong type. Learn more: {HEADER_DOCS_URL}.")]
    VertexShaderBadInput(#[source] TypeEquivalenceError),

    #[error("User defined binding (group {USER_DEFINED_BUFFER_GROUP}, binding {USER_DEFINED_BUFFER_BINDING}) is not a uniform buffer. Is it defined as var<uniform>?")]
    UserBindingNotUniform,
}

#[derive(Debug, thiserror::Error)]
pub enum TypeEquivalenceError {
    #[error("Type names don't match (expected: {expected}, actual: {actual}).")]
    TypeNameMismatch { expected: String, actual: String },

    #[error("Type mismatch (expected: {expected}, actual: {actual}).")]
    TypeStructureMismatch { expected: String, actual: String },

    #[error("Struct \"{struct_name}\" has an incorrect number of fields: expected: {expected_field_number}, found: {actual_field_number}")]
    StructFieldNumberMismatch {
        struct_name: String,
        expected_field_number: usize,
        actual_field_number: usize,
    },

    #[error("Field \"{field_name}\" in struct \"{struct_name}\" has an invalid type.")]
    StructFieldStructureMismatch {
        struct_name: String,
        field_name: String,
        #[source]
        error: Box<TypeEquivalenceError>,
    },

    #[error("Struct {struct_name} has mismatched field names: expected {expected_field_name}, found: {actual_field_name}.")]
    StructFieldNameMismatch {
        struct_name: String,
        expected_field_name: String,
        actual_field_name: String,
    },

    #[error("Field {field_name} in struct {struct_name} has an incorrect binding: expected \"{expected_binding} {field_name}\", found \"{actual_binding} {field_name}\".")]
    StructFieldBindingMismatch {
        struct_name: String,
        field_name: String,
        expected_binding: String,
        actual_binding: String,
    },

    #[error(transparent)]
    BadArraySize(#[from] ConstArraySizeEvalError),

    #[error("Sizes of an array don't match: {0:?} != {1:?}.")]
    ArraySizeMismatch(u64, u64),
}

#[derive(Debug, thiserror::Error)]
pub enum ConstArraySizeEvalError {
    #[error("Dynamic array size is not allowed.")]
    DynamicSize,
}

#[derive(Debug, thiserror::Error)]
pub enum ParametersValidationError {
    #[error("No user-defined binding was found in the shader, even though parameters were provided in the request. Add \"@group(1) @binding(0) var<uniform> example_params: ExampleType;\" in your shader code.")]
    NoBindingInShader,

    #[error("A type used in the shader cannot be provided at node registration: {0}.")]
    ForbiddenType(&'static str),

    #[error("An unsupported scalar kind.")]
    UnsupportedScalarKind(naga::ScalarKind, u8),

    #[error("Type mismatch (expected: {expected}, actual: {actual}).")]
    WrongType { expected: String, actual: String },

    #[error("A list of parameters is too long (expected: {expected}, actual: {actual}).")]
    ListTooLong { expected: usize, actual: usize },

    #[error("Error while evaluating array size")]
    ArraySizeEvalError(#[from] ConstArraySizeEvalError),

    #[error("Struct \"{struct_name}\" has {expected} field(s), but {actual} were provided via shader parameters.")]
    WrongShaderFieldsAmount {
        struct_name: String,
        expected: usize,
        actual: usize,
    },

    #[error("The field at index {index} in struct \"{struct_name}\" is named \"{expected}\", but \"{actual}\" were provided via shader parameters.")]
    WrongFieldName {
        index: usize,
        struct_name: String,
        expected: String,
        actual: String,
    },

    #[error("Error while verifying field \"{struct_field}\" in struct \"{struct_name}\".")]
    WrongFieldType {
        struct_name: String,
        struct_field: String,
        #[source]
        error: Box<ParametersValidationError>,
    },

    #[error("Error while verifying array element at index {idx}.")]
    WrongArrayElementType {
        idx: usize,
        #[source]
        error: Box<ParametersValidationError>,
    },

    #[error("Error while verifying vector element at index {idx}.")]
    WrongVectorElementType {
        idx: usize,
        #[source]
        error: Box<ParametersValidationError>,
    },

    #[error("Error while verifying matrix row {idx}.")]
    WrongMatrixRowType {
        idx: usize,
        #[source]
        error: Box<ParametersValidationError>,
    },
}

pub(crate) trait ShaderGlobalVariableExt {
    fn to_string(&self) -> String;
}

impl ShaderGlobalVariableExt for naga::GlobalVariable {
    fn to_string(&self) -> String {
        let group_and_binding = match self.binding {
            Some(naga::ResourceBinding { group, binding }) => {
                format!("@group({}) @binding({}) ", group, binding)
            }
            None => "".to_string(),
        };
        let space = match self.space {
            naga::AddressSpace::Function => "<function>".to_string(),
            naga::AddressSpace::Private => "<private>".to_string(),
            naga::AddressSpace::WorkGroup => "<workgroup>".to_string(),
            naga::AddressSpace::Uniform => "<uniform>".to_string(),
            naga::AddressSpace::Storage { access } => {
                let access = match access.bits() {
                    1 => "read",
                    2 => "write",
                    3 => "read_write",
                    _ => "",
                };
                format!("<storage, {}>", access)
            }
            naga::AddressSpace::Handle => "".to_string(),
            naga::AddressSpace::PushConstant => "<push_constant>".to_string(),
        };
        let name = self.name.clone().unwrap_or("value".to_string());
        format!("{group_and_binding}var{space} {name}")
    }
}

pub(crate) trait BindingExt {
    fn to_string(&self) -> String;
}

impl BindingExt for naga::Binding {
    fn to_string(&self) -> String {
        match self {
            naga::Binding::BuiltIn(builtin) => format!("{:?}", builtin),
            // default interpolation and sampling depends on type, so it's hard to detect if
            // provided value is a default or not. For now we are just printing location.
            // If we ever start using @interpolate in header this implementation needs to be
            // updated.
            naga::Binding::Location { location, .. } => format!("@location({})", location),
        }
    }
}
