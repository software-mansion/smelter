mod header_checking {
    use super::super::*;

    #[test]
    fn array_len() {
        let expected = r#"
            var a: array<i32, 16>;
            "#;

        let provided = r#"
            var a: array<i32, 17>;
            "#;

        let expected = naga::front::wgsl::parse_str(expected).unwrap();
        let provided = naga::front::wgsl::parse_str(provided).unwrap();

        assert!(matches!(
            validate_contains_header(&expected, &provided),
            Err(ShaderValidationError::GlobalBadType(_, _))
        ));
    }

    #[test]
    fn binding() {
        let expected = r#"
            @group(0) @binding(0) var a: i32;
            "#;

        let provided = r#"
            @group(0) @binding(1) var a: i32;
            "#;

        let expected = naga::front::wgsl::parse_str(expected).unwrap();
        let provided = naga::front::wgsl::parse_str(provided).unwrap();

        assert!(matches!(
            validate_contains_header(&expected, &provided),
            Err(ShaderValidationError::GlobalNotFound(_))
        ));
    }

    #[test]
    fn vertex_input() {
        let expected = r#"
            struct VertexInput {
                @location(0) position: vec3<f32>,
                @location(1) tex_coords: vec2<f32>,
            }
            "#;

        let provided = r#"
            struct VertexInput {
                @location(0) position: vec3<f32>,
                @location(1) tex_coords: vec2<u32>,
            }
    
            @vertex
            fn vs_main(in: VertexInput) -> @builtin(position) vec4<f32> {
                return vec4(0);
            }
            "#;

        let expected = naga::front::wgsl::parse_str(expected).unwrap();
        let provided = naga::front::wgsl::parse_str(provided).unwrap();

        assert!(matches!(
            validate_contains_header(&expected, &provided),
            Err(ShaderValidationError::VertexShaderBadInput(_))
        ));
    }

    #[test]
    fn vertex_input_locations() {
        let expected = r#"
            struct VertexInput {
                @location(0) position: vec3<f32>,
                @location(1) tex_coords: vec2<f32>,
            }
            "#;

        let provided = r#"
            struct VertexInput {
                @location(0) position: vec3<f32>,
                @location(2) tex_coords: vec2<f32>,
            }
    
            @vertex
            fn vs_main(in: VertexInput) -> @builtin(position) vec4<f32> {
                return vec4(0);
            }
            "#;

        let expected = naga::front::wgsl::parse_str(expected).unwrap();
        let provided = naga::front::wgsl::parse_str(provided).unwrap();

        assert!(matches!(
            validate_contains_header(&expected, &provided),
            Err(ShaderValidationError::VertexShaderBadInput(_))
        ));
    }
}

mod params_validation {
    use crate::scene::ShaderParamStructField;

    use super::super::*;

    fn parse_and_get_type(shader: &str, type_name: &str) -> (naga::Module, Handle<Type>) {
        let module = naga::front::wgsl::parse_str(shader).unwrap();

        let ty = get_type(type_name, &module);

        (module, ty)
    }

    fn get_type(name: &str, module: &naga::Module) -> Handle<Type> {
        module
            .types
            .iter()
            .find(|(_, ty)| ty.name == Some(name.to_string()))
            .map(|(handle, _)| handle)
            .unwrap()
    }

    #[test]
    fn big() {
        let (module, ty) = parse_and_get_type(
            r#"
                struct MyType2 {
                    int: i32,
                }

                struct MyType {
                    int: i32,
                    uint: u32,
                    list: array<MyType2, 2>
                }
            "#,
            "MyType",
        );

        let params = ShaderParam::Struct(vec![
            ShaderParamStructField {
                field_name: "int".into(),
                value: ShaderParam::I32(-5),
            },
            ShaderParamStructField {
                field_name: "uint".into(),
                value: ShaderParam::U32(5),
            },
            ShaderParamStructField {
                field_name: "list".into(),
                value: ShaderParam::List(vec![ShaderParam::Struct(vec![ShaderParamStructField {
                    field_name: "int".into(),
                    value: ShaderParam::I32(-10),
                }])]),
            },
        ]);

        validate_params(&params, ty, &module).unwrap();
    }

    #[test]
    fn vec_len() {
        let (module, ty) = parse_and_get_type(
            r#"
                    struct MyType {
                        vec: vec3<f32>
                    }
                "#,
            "MyType",
        );

        let params = ShaderParam::Struct(vec![ShaderParamStructField {
            field_name: "vec".into(),
            value: ShaderParam::List(vec![ShaderParam::F32(1.0), ShaderParam::F32(2.0)]),
        }]);

        assert!(matches!(
            validate_params(&params, ty, &module),
            Err(ParametersValidationError::WrongFieldType { .. })
        ))
    }

    #[test]
    fn field_name() {
        let (module, ty) = parse_and_get_type(
            r#"
                    struct MyType {
                        field: i32
                    }
                "#,
            "MyType",
        );

        let params = ShaderParam::Struct(vec![ShaderParamStructField {
            field_name: "wrong_name".into(),
            value: ShaderParam::I32(5),
        }]);

        assert!(matches!(
            validate_params(&params, ty, &module),
            Err(ParametersValidationError::WrongFieldName { .. })
        ))
    }
}
