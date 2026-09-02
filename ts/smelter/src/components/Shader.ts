import type * as Api from '../api.js';
import type { ComponentBaseProps, SceneComponent } from '../component.js';
import { createSmelterComponent, sceneComponentIntoApi } from '../component.js';

export type ShaderProps = ComponentBaseProps & {
  /**
   * Id of a shader. It identifies a shader registered using `Smelter.registerShader`.
   */
  shaderId: Api.RendererId;
  /**
   * Object that will be serialized into a `struct` and passed inside the shader as:
   *
   * ```wgsl
   * @group(1) @binding(0) var<uniform>
   * ```
   *
   * Note: this object's structure must match the structure defined in a shader source code.
   * Currently, we do not handle memory layout automatically. To achieve the correct memory
   * alignment, you might need to pad your data with additional fields. See
   * [WGSL documentation](https://www.w3.org/TR/WGSL/#alignment-and-size) for more details.
   */
  shaderParam?: ShaderParam;
  /**
   * Resolution of a texture where shader will be executed.
   */
  resolution: Api.Resolution;
};

export type ShaderParam =
  | { type: 'f32'; value: number }
  | { type: 'u32'; value: number }
  | { type: 'i32'; value: number }
  | { type: 'list'; value: ShaderParam[] }
  | {
      type: 'struct';
      value: ShaderParamStructField[];
    };

export type ShaderParamStructField =
  | { type: 'f32'; value: number; fieldName: string }
  | { type: 'u32'; value: number; fieldName: string }
  | { type: 'i32'; value: number; fieldName: string }
  | {
      type: 'list';
      value: ShaderParam[];
      fieldName: string;
    }
  | {
      type: 'struct';
      value: ShaderParamStructField[];
      fieldName: string;
    };

const Shader = createSmelterComponent<ShaderProps>(sceneBuilder);

function sceneBuilder(props: ShaderProps, children: SceneComponent[]): Api.Component {
  return {
    type: 'shader',
    children: children.map(sceneComponentIntoApi),
    id: props.id,
    shader_id: props.shaderId,
    shader_param: props.shaderParam && intoShaderParams(props.shaderParam),
    resolution: props.resolution,
  };
}

function intoShaderParams(param: ShaderParam): Api.ShaderParam {
  if (param.type === 'f32') {
    return {
      type: 'f32',
      value: param.value,
    };
  } else if (param.type === 'u32') {
    return {
      type: 'u32',
      value: param.value,
    };
  } else if (param.type === 'i32') {
    return {
      type: 'i32',
      value: param.value,
    };
  } else if (param.type === 'list') {
    return {
      type: 'list',
      value: param.value.map(intoShaderParams),
    };
  } else if (param.type === 'struct') {
    return {
      type: 'struct',
      value: param.value.map(intoShaderStructField),
    };
  } else {
    throw new Error('Invalid shader params');
  }
}

function intoShaderStructField(param: ShaderParamStructField): Api.ShaderParamStructField {
  if (param.type === 'f32') {
    return {
      type: 'f32',
      value: param.value,
      field_name: param.fieldName,
    };
  } else if (param.type === 'u32') {
    return {
      type: 'u32',
      value: param.value,
      field_name: param.fieldName,
    };
  } else if (param.type === 'i32') {
    return {
      type: 'i32',
      value: param.value,
      field_name: param.fieldName,
    };
  } else if (param.type === 'list') {
    return {
      type: 'list',
      value: param.value.map(intoShaderParams),
      field_name: param.fieldName,
    };
  } else if (param.type === 'struct') {
    return {
      type: 'struct',
      value: param.value.map(intoShaderStructField),
      field_name: param.fieldName,
    };
  } else {
    throw new Error('Invalid shader params');
  }
}

export default Shader;
