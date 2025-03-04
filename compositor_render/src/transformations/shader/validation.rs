use naga::{ArraySize, Handle, Module, ScalarKind, ShaderStage, Type, VectorSize};

use crate::scene::ShaderParam;

pub mod error;

use error::{
    BindingExt, ConstArraySizeEvalError, ParametersValidationError, ShaderGlobalVariableExt,
    ShaderValidationError, TypeEquivalenceError,
};

pub fn shader_header() -> Module {
    let header_code = match cfg!(target_arch = "wasm32") {
        false => include_str!("./validation/shader_header.wgsl"),
        true => include_str!("./validation/shader_header_web.wgsl"),
    };

    naga::front::wgsl::parse_str(header_code).expect("failed to parse the shader header file")
}

pub(super) fn validate_contains_header(
    header: &naga::Module,
    shader: &naga::Module,
) -> Result<(), ShaderValidationError> {
    validate_globals(header, shader)?;
    validate_vertex_input(header, shader)?;
    Ok(())
}

fn validate_globals(
    header: &naga::Module,
    shader: &naga::Module,
) -> Result<(), ShaderValidationError> {
    for (_, global) in header.global_variables.iter() {
        let (_, global_in_shader) = shader
            .global_variables
            .iter()
            .find(|(_, s_global)| {
                s_global.space == global.space && s_global.binding == global.binding
            })
            .ok_or_else(|| ShaderValidationError::GlobalNotFound(global.to_string()))?;

        validate_type_equivalent(global.ty, header, global_in_shader.ty, shader).map_err(
            |err| {
                ShaderValidationError::GlobalBadType(
                    err,
                    global_in_shader.name.unwrap_with("<unknown>"),
                )
            },
        )?;
    }

    // validate user-defined buffer is a uniform
    shader
        .global_variables
        .iter()
        .find(|(_, global)| {
            global.binding.is_some()
                && global.binding.as_ref().unwrap().group
                    == super::pipeline::USER_DEFINED_BUFFER_GROUP
                && global.binding.as_ref().unwrap().binding
                    == super::pipeline::USER_DEFINED_BUFFER_BINDING
                && global.space == naga::AddressSpace::Uniform
        })
        .map_or(Ok(()), |(_, global)| match global.space {
            naga::AddressSpace::Uniform => Ok(()),
            _ => Err(ShaderValidationError::UserBindingNotUniform),
        })?;

    Ok(())
}

fn validate_vertex_input(
    header: &naga::Module,
    shader: &naga::Module,
) -> Result<(), ShaderValidationError> {
    let vertex = shader
        .entry_points
        .iter()
        .find(|entry_point| {
            entry_point.name == crate::wgpu::common_pipeline::VERTEX_ENTRYPOINT_NAME
                && entry_point.stage == ShaderStage::Vertex
        })
        .ok_or(ShaderValidationError::VertexShaderNotFound)?;

    if vertex.function.arguments.len() != 1 {
        return Err(ShaderValidationError::VertexShaderBadArgumentAmount(
            vertex.function.arguments.len(),
        ));
    }

    let vertex_input = vertex.function.arguments[0].ty;
    let vertex_input_type = &shader.types[vertex_input];

    let (header_vertex_input, _) = header
        .types
        .iter()
        .find(|(_, ty)| ty.name == vertex_input_type.name)
        .ok_or_else(|| {
            ShaderValidationError::VertexShaderBadInputTypeName(
                vertex_input_type.name.unwrap_with("<unknown>"),
            )
        })?;

    validate_type_equivalent(header_vertex_input, header, vertex_input, shader)
        .map_err(ShaderValidationError::VertexShaderBadInput)?;

    Ok(())
}

fn validate_type_equivalent(
    expected: Handle<Type>,
    expected_module: &Module,
    provided: Handle<Type>,
    provided_module: &Module,
) -> Result<(), TypeEquivalenceError> {
    let expected_type = &expected_module.types[expected];
    let provided_type = &provided_module.types[provided];

    if expected_type.name != provided_type.name && expected_type.name.is_some() {
        return Err(TypeEquivalenceError::TypeNameMismatch {
            expected: expected_type.name.unwrap_with("<unknown>"),
            actual: provided_type.name.unwrap_with("<unknown>"),
        });
    }

    let expected_inner = match expected_type.inner.canonical_form(&expected_module.types) {
        Some(t) => t,
        None => expected_type.inner.clone(),
    };
    let provided_inner = match provided_type.inner.canonical_form(&provided_module.types) {
        Some(t) => t,
        None => provided_type.inner.clone(),
    };

    match expected_inner {
        naga::TypeInner::Scalar { .. }
        | naga::TypeInner::Vector { .. }
        | naga::TypeInner::Matrix { .. }
        | naga::TypeInner::Atomic { .. }
        | naga::TypeInner::Image { .. }
        | naga::TypeInner::Sampler { .. }
        | naga::TypeInner::AccelerationStructure
        | naga::TypeInner::RayQuery
        | naga::TypeInner::ValuePointer { .. } => {
            if expected_inner != provided_inner {
                return Err(TypeEquivalenceError::TypeStructureMismatch {
                    expected: expected_type.inner.to_string(expected_module),
                    actual: provided_type.inner.to_string(provided_module),
                });
            }
        }

        naga::TypeInner::Array {
            base: expected_base,
            size: expected_size,
            stride: expected_stride,
        } => {
            let naga::TypeInner::Array {
                base: provided_base,
                size: provided_size,
                stride: provided_stride,
            } = provided_inner
            else {
                return Err(TypeEquivalenceError::TypeStructureMismatch {
                    expected: expected_inner.to_string(expected_module),
                    actual: provided_inner.to_string(provided_module),
                });
            };

            if expected_stride != provided_stride {
                return Err(TypeEquivalenceError::TypeStructureMismatch {
                    expected: expected_inner.to_string(expected_module),
                    actual: provided_inner.to_string(provided_module),
                });
            }

            validate_array_size_equivalent(expected_size, provided_size)?;
            return validate_type_equivalent(
                expected_base,
                expected_module,
                provided_base,
                provided_module,
            );
        }

        naga::TypeInner::BindingArray {
            base: expected_base,
            size: expected_size,
        } => {
            let naga::TypeInner::BindingArray {
                base: provided_base,
                size: provided_size,
            } = provided_inner
            else {
                return Err(TypeEquivalenceError::TypeStructureMismatch {
                    expected: expected_inner.to_string(expected_module),
                    actual: provided_inner.to_string(provided_module),
                });
            };

            validate_array_size_equivalent(expected_size, provided_size)?;
            return validate_type_equivalent(
                expected_base,
                expected_module,
                provided_base,
                provided_module,
            );
        }

        naga::TypeInner::Struct {
            members: ref expected_members,
            ..
        } => {
            let naga::TypeInner::Struct {
                members: ref provided_members,
                ..
            } = provided_inner
            else {
                return Err(TypeEquivalenceError::TypeStructureMismatch {
                    expected: expected_inner.to_string(expected_module),
                    actual: provided_inner.to_string(provided_module),
                });
            };

            // skipped checking if ti1.span == ti2.span
            // if all fields have the same types, how can the spans be different?

            if expected_members.len() != provided_members.len() {
                return Err(TypeEquivalenceError::StructFieldNumberMismatch {
                    struct_name: expected_type.name.unwrap_with("<unnamed>"),
                    expected_field_number: expected_members.len(),
                    actual_field_number: provided_members.len(),
                });
            }

            for (expected_member, provided_member) in
                expected_members.iter().zip(provided_members.iter())
            {
                if expected_member.name != provided_member.name {
                    return Err(TypeEquivalenceError::StructFieldNameMismatch {
                        struct_name: expected_type.name.unwrap_with("<unnamed>"),
                        expected_field_name: expected_member.name.unwrap_with("<unnamed>"),
                        actual_field_name: provided_member.name.unwrap_with("<unnamed>"),
                    });
                }

                // skipped checking if m1.offset == m2.offset
                // if all fields have the same types, how can the offsets be different?

                validate_type_equivalent(
                    expected_member.ty,
                    expected_module,
                    provided_member.ty,
                    provided_module,
                )
                .map_err(|err| {
                    TypeEquivalenceError::StructFieldStructureMismatch {
                        struct_name: expected_type.name.unwrap_with("<unnamed>"),
                        field_name: expected_member.name.unwrap_with("<unnamed>"),
                        error: Box::new(err),
                    }
                })?;

                if expected_member.binding != provided_member.binding {
                    return Err(TypeEquivalenceError::StructFieldBindingMismatch {
                        struct_name: expected_type.name.unwrap_with("<unnamed>"),
                        field_name: expected_member.name.unwrap_with("<unnamed>"),
                        expected_binding: expected_member
                            .binding
                            .as_ref()
                            .map(BindingExt::to_string)
                            .unwrap_with("<no binding>"),
                        actual_binding: provided_member
                            .binding
                            .as_ref()
                            .map(BindingExt::to_string)
                            .unwrap_with("<no binding>"),
                    });
                }
            }
        }

        naga::TypeInner::Pointer { .. } => {
            panic!("this should never happen bc of canonicalization")
        }
    }

    Ok(())
}

fn eval_array_size(size: ArraySize) -> Result<u64, ConstArraySizeEvalError> {
    match size {
        ArraySize::Constant(c) => Ok(c.get().into()),
        ArraySize::Dynamic => Err(ConstArraySizeEvalError::DynamicSize),
        ArraySize::Pending(_) => Err(ConstArraySizeEvalError::OverrideSize),
    }
}

fn validate_array_size_equivalent(
    expected_size: ArraySize,
    provided_size: ArraySize,
) -> Result<(), TypeEquivalenceError> {
    let expected_size = eval_array_size(expected_size)?;
    let provided_size = eval_array_size(provided_size)?;

    if expected_size != provided_size {
        return Err(TypeEquivalenceError::ArraySizeMismatch(
            expected_size,
            provided_size,
        ));
    }

    Ok(())
}

pub(super) fn validate_params(
    params: &ShaderParam,
    ty: Handle<Type>,
    module: &naga::Module,
) -> Result<(), ParametersValidationError> {
    let ty = &module.types[ty];

    match &ty.inner {
        naga::TypeInner::Scalar(scalar) => validate_scalar(params, *scalar),

        naga::TypeInner::Vector { size, scalar } => validate_vector(params, *size, *scalar, module),

        naga::TypeInner::Matrix {
            columns,
            rows,
            scalar,
        } => validate_matrix(params, *columns, *rows, *scalar, module),

        naga::TypeInner::Array { base, size, stride } => {
            validate_array(params, *base, *size, *stride, module)
        }

        naga::TypeInner::Struct { members, span } => validate_struct(
            params,
            ty.name.as_deref().unwrap_or("<unnamed>"),
            members,
            *span,
            module,
        ),

        naga::TypeInner::Pointer { .. }
        | naga::TypeInner::ValuePointer { .. }
        | naga::TypeInner::Atomic { .. }
        | naga::TypeInner::Image { .. }
        | naga::TypeInner::Sampler { .. }
        | naga::TypeInner::AccelerationStructure
        | naga::TypeInner::RayQuery
        | naga::TypeInner::BindingArray { .. } => Err(ParametersValidationError::ForbiddenType(
            ty.inner.type_name(),
        )),
    }
}

fn validate_struct(
    params: &ShaderParam,
    struct_name_in_shader: &str,
    struct_members_in_shader: &[naga::StructMember],
    span: u32,
    module: &naga::Module,
) -> Result<(), ParametersValidationError> {
    match params {
        ShaderParam::Struct(param_fields) => {
            if struct_members_in_shader.len() != param_fields.len() {
                return Err(ParametersValidationError::WrongShaderFieldsAmount {
                    struct_name: struct_name_in_shader.to_string(),
                    expected: struct_members_in_shader.len(),
                    actual: param_fields.len(),
                });
            }

            for (index, (shader_member, param_field)) in struct_members_in_shader
                .iter()
                .zip(param_fields.iter())
                .enumerate()
            {
                if shader_member.name.as_deref().unwrap_or("<unnamed>") != param_field.field_name {
                    return Err(ParametersValidationError::WrongFieldName {
                        index,
                        struct_name: struct_name_in_shader.into(),
                        expected: shader_member.name.unwrap_with("<unnamed>"),
                        actual: param_field.field_name.clone(),
                    });
                }

                validate_params(&param_field.value, shader_member.ty, module).map_err(|err| {
                    ParametersValidationError::WrongFieldType {
                        struct_name: struct_name_in_shader.into(),
                        struct_field: param_field.field_name.clone(),
                        error: Box::new(err),
                    }
                })?
            }

            Ok(())
        }

        _ => Err(ParametersValidationError::WrongType {
            actual: params.to_string(),
            expected: naga::TypeInner::Struct {
                members: struct_members_in_shader.to_owned(),
                span,
            }
            .to_string(module),
        }),
    }
}

fn validate_array(
    params: &ShaderParam,
    base: Handle<Type>,
    size: ArraySize,
    stride: u32,
    module: &naga::Module,
) -> Result<(), ParametersValidationError> {
    // ignoring the `stride`, it probably doesn't matter if the types are correct
    let evaluated_size = eval_array_size(size)?;

    match params {
        ShaderParam::List(list) => {
            if list.len() > evaluated_size as usize {
                return Err(ParametersValidationError::ListTooLong {
                    expected: evaluated_size as usize,
                    actual: list.len(),
                });
            }

            for (idx, param) in list.iter().enumerate() {
                validate_params(param, base, module).map_err(|err| {
                    ParametersValidationError::WrongArrayElementType {
                        idx,
                        error: Box::new(err),
                    }
                })?
            }

            Ok(())
        }

        _ => Err(ParametersValidationError::WrongType {
            actual: params.to_string(),
            expected: naga::TypeInner::Array { base, size, stride }.to_string(module),
        }),
    }
}

fn validate_matrix(
    params: &ShaderParam,
    columns: VectorSize,
    rows: VectorSize,
    scalar: naga::Scalar,
    module: &naga::Module,
) -> Result<(), ParametersValidationError> {
    match params {
        ShaderParam::List(rows_list) => {
            if rows_list.len() != rows as usize {
                return Err(ParametersValidationError::ListTooLong {
                    expected: rows as usize,
                    actual: rows_list.len(),
                });
            }

            for (idx, row) in rows_list.iter().enumerate() {
                validate_vector(row, columns, scalar, module).map_err(|err| {
                    ParametersValidationError::WrongMatrixRowType {
                        idx,
                        error: Box::new(err),
                    }
                })?
            }

            Ok(())
        }

        _ => Err(ParametersValidationError::WrongType {
            actual: params.to_string(),
            expected: naga::TypeInner::Matrix {
                columns,
                rows,
                scalar,
            }
            .to_string(module),
        }),
    }
}

fn validate_vector(
    params: &ShaderParam,
    size: VectorSize,
    scalar: naga::Scalar,
    module: &naga::Module,
) -> Result<(), ParametersValidationError> {
    match params {
        ShaderParam::List(list) => {
            if list.len() != size as usize {
                return Err(ParametersValidationError::ListTooLong {
                    expected: size as usize,
                    actual: list.len(),
                });
            }

            for (idx, v) in list.iter().enumerate() {
                validate_scalar(v, scalar).map_err(|err| {
                    ParametersValidationError::WrongVectorElementType {
                        idx,
                        error: Box::new(err),
                    }
                })?
            }

            Ok(())
        }

        _ => Err(ParametersValidationError::WrongType {
            actual: params.to_string(),
            expected: naga::TypeInner::Vector { size, scalar }.to_string(module),
        }),
    }
}

fn validate_scalar(
    params: &ShaderParam,
    scalar: naga::Scalar,
) -> Result<(), ParametersValidationError> {
    match scalar {
        naga::Scalar {
            kind: ScalarKind::Float,
            width: 4,
        } => match params {
            ShaderParam::F32(_) => Ok(()),
            _ => Err(ParametersValidationError::WrongType {
                actual: params.to_string(),
                expected: scalar.to_string(),
            }),
        },

        naga::Scalar {
            kind: ScalarKind::Uint,
            width: 4,
        } => match params {
            ShaderParam::U32(_) => Ok(()),
            _ => Err(ParametersValidationError::WrongType {
                actual: params.to_string(),
                expected: scalar.to_string(),
            }),
        },

        naga::Scalar {
            kind: ScalarKind::Sint,
            width: 4,
        } => match params {
            ShaderParam::I32(_) => Ok(()),
            _ => Err(ParametersValidationError::WrongType {
                actual: params.to_string(),
                expected: scalar.to_string(),
            }),
        },

        _ => Err(ParametersValidationError::UnsupportedScalarKind(
            scalar.kind,
            scalar.width,
        )),
    }
}

trait TypeInnerExt {
    fn type_name(&self) -> &'static str;
    fn to_string(&self, module: &naga::Module) -> String;
}

impl TypeInnerExt for naga::TypeInner {
    fn type_name(&self) -> &'static str {
        match self {
            naga::TypeInner::Scalar { .. } => "scalar",
            naga::TypeInner::Vector { .. } => "vector",
            naga::TypeInner::Matrix { .. } => "matrix",
            naga::TypeInner::Atomic { .. } => "atomic",
            naga::TypeInner::Pointer { .. } => "pointer",
            naga::TypeInner::ValuePointer { .. } => "value pointer",
            naga::TypeInner::Array { .. } => "array",
            naga::TypeInner::Struct { .. } => "struct",
            naga::TypeInner::Image { .. } => "texture",
            naga::TypeInner::Sampler { .. } => "sampler",
            naga::TypeInner::AccelerationStructure => "acceleration structure",
            naga::TypeInner::RayQuery => "ray query",
            naga::TypeInner::BindingArray { .. } => "binding array",
        }
    }
    fn to_string(&self, module: &naga::Module) -> String {
        match self {
            naga::TypeInner::Scalar(scalar) => scalar.to_string(),
            naga::TypeInner::Vector { size, scalar } => {
                format!("vec{}<{}>", *size as u8, scalar.to_string())
            }
            naga::TypeInner::Matrix { .. } => "matrix".to_string(),
            naga::TypeInner::Atomic { .. } => "atomic".to_string(),
            naga::TypeInner::Pointer { .. } => "pointer".to_string(),
            naga::TypeInner::ValuePointer { .. } => "value pointer".to_string(),
            naga::TypeInner::Array { .. } => "array".to_string(),
            naga::TypeInner::Struct { .. } => "struct".to_string(),
            naga::TypeInner::Image {
                dim,
                arrayed,
                class,
            } => {
                let fallback = format!("{:?}", self);
                let naga::ImageClass::Sampled { kind, .. } = class else {
                    return fallback;
                };

                let scalar = naga::Scalar {
                    kind: *kind,
                    width: 4,
                }
                .to_string();

                let texture_kind = match (dim, arrayed) {
                    (naga::ImageDimension::D1, false) => "texture_1d",
                    (naga::ImageDimension::D2, false) => "texture_2d",
                    (naga::ImageDimension::D2, true) => "texture_2d_array",
                    (naga::ImageDimension::D3, false) => "texture_3d",
                    (naga::ImageDimension::Cube, false) => "texture_cube",
                    (naga::ImageDimension::Cube, true) => "texture_cube_array",
                    _ => return fallback,
                };

                format!("{texture_kind}<{scalar}>")
            }
            naga::TypeInner::Sampler { .. } => "sampler".to_string(),
            naga::TypeInner::AccelerationStructure => "acceleration structure".to_string(),
            naga::TypeInner::RayQuery => "ray query".to_string(),
            naga::TypeInner::BindingArray { base, size } => {
                let size: Option<u32> = match size {
                    ArraySize::Constant(size) => Some(size.get()),
                    ArraySize::Dynamic => None, // TODO: not sure how to handle this
                    ArraySize::Pending(_) => None, // TODO: not sure how to handle this
                };
                let base: &naga::Type = &module.types[*base];
                format!(
                    "binding_array<{}, {}>",
                    base.inner.to_string(module),
                    size.map(|s| s.to_string()).unwrap_or("_".to_string())
                )
            }
        }
    }
}

trait ScalarExt {
    fn to_string(&self) -> String;
}

impl ScalarExt for naga::Scalar {
    fn to_string(&self) -> String {
        match self.kind {
            ScalarKind::Sint => format!("i{}", self.width * 8),
            ScalarKind::Uint => format!("u{}", self.width * 8),
            ScalarKind::Float => format!("f{}", self.width * 8),
            ScalarKind::Bool => "bool".to_string(),
            ScalarKind::AbstractInt => "abstract int".into(),
            ScalarKind::AbstractFloat => "abstract float".into(),
        }
    }
}

trait ShaderParamExt {
    fn to_string(&self) -> String;
}

impl ShaderParamExt for ShaderParam {
    fn to_string(&self) -> String {
        match self {
            ShaderParam::F32(_) => "f32".to_string(),
            ShaderParam::U32(_) => "u32".to_string(),
            ShaderParam::I32(_) => "i32".to_string(),
            ShaderParam::List(list) => {
                let list = list
                    .iter()
                    .map(|field| field.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("[{}]", list)
            }
            ShaderParam::Struct(fields) => {
                let fields = fields
                    .iter()
                    .map(|field| format!("{}: {}", field.field_name, field.value.to_string()))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("struct {{ {} }}", fields)
            }
        }
    }
}

trait OptionUnwrapExt {
    fn unwrap_with(self, fallback: &'static str) -> String;
}

impl OptionUnwrapExt for &Option<String> {
    fn unwrap_with(self, fallback: &'static str) -> String {
        self.clone().unwrap_or_else(|| fallback.to_string())
    }
}

#[cfg(test)]
mod tests;
