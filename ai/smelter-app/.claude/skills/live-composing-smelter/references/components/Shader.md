# Shader Component

Renders output of a user-provided WGSL shader. Children components are available as textures inside the shader. Requires pre-registration via `smelter.registerShader()`.

> **WASM note**: When using `@swmansion/smelter-web-wasm`, shaders accept only ONE texture (`texture_2d<f32>` instead of `binding_array<texture_2d<f32>, 16>`).

## Type Definition

```tsx
type ShaderProps = {
  id?: string;
  children?: ReactElement[];
  shaderId: string;
  shaderParam?: ShaderParam;
  resolution: {
    width: number;
    height: number;
  };
}
```

## Props

### shaderId (required)
ID matching a shader registered via `smelter.registerShader()`.
- **Type**: `string`

### resolution (required)
Texture resolution for shader execution.
- **Type**: `{ width: number; height: number }`

### children
Child components provided as textures to the shader.
- **Type**: `ReactElement[]`

### id
Component ID.
- **Type**: `string`
- **Default**: Value from `useId` hook

### shaderParam
Struct passed to the shader as `@group(1) @binding(0) var<uniform>`. Must match the structure defined in shader source. Memory layout must be handled manually.
- **Type**: `ShaderParam`

## ShaderParam Types

```tsx
type ShaderParam =
  | { type: "f32"; value: number; }
  | { type: "u32"; value: number; }
  | { type: "i32"; value: number; }
  | { type: "list"; value: ShaderParam[]; }
  | { type: "struct"; value: ShaderParamStructField[]; }

type ShaderParamStructField =
  | { field_name: string; type: "f32"; value: number; }
  | { field_name: string; type: "u32"; value: number; }
  | { field_name: string; type: "i32"; value: number; }
  | { field_name: string; type: "list"; value: ShaderParam[]; }
  | { field_name: string; type: "struct"; value: ShaderParamStructField[]; }
```

> Memory alignment must be managed manually. Add padding fields as needed per WGSL spec.
