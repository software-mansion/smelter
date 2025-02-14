# Shaders

Shaders are small programs that we send to a GPU to perform some computation for us. They are used extensively in the LiveCompositor. All builtin transformation are implemented as shaders under the hood. It is also possible to create render nodes that run a custom shader on their input. Since LiveCompositor is implemented using wgpu, the shaders have to be written in WGSL (WebGPU Shading Language). They also have to fulfill some custom requirements that allow them to be run by the LiveCompositor.

## General concepts

There are two kinds of shaders that are used in the LiveCompositor: vertex shaders and fragment shaders.

### Vertex shaders

Vertex shaders receive the data of a single vertex as input. It can manipulate them to make them form the shape we want to see as the output.

The videos are represented in vertex shaders as two triangles, aligned like so:

```console
 ______
|     /|
|    / |
|   /  |
|  /   |
| /    |
|______|
```

The rectangle formed by these triangles spans the whole clip space, i.e. [-1, 1] X [-1, 1].

Each video passed in as input gets a separate rectangle.
`plane_id` in `base_params` `push_constant` represents number of currently rendered plane (texture).

If there are no input textures, `plane_id` is equal to -1 and a single rectangle is passed to the shader. It is only useful for shaders that generate something in the fragment shader.

Since the compositor doesn't deal with complex geometry and most positioning/resizing/cropping should be taken care of by [layouts](https://smelter.dev/docs/concept/layouts), we don't expect the users to write nontrivial vertex shaders very often. For just applying some effects to the video, fragment shaders are the way to go. This vertex shader should take care of most of your needs (for transformations that receive a single video and only process it in the fragment shader):

```wgsl
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
}

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;

    output.position = vec4(input.position, 1.0);
    output.tex_coords = input.tex_coords;

    return output;
}
```

### Fragment shaders

A single instance of a fragment shader is started for each pixel of the output texture. That instance is responsible for calculating the color of the pixel. The return type of a fragment shader has to be a 4-element long vector of `f32`s ranging from 0.0 to 1.0. These floats are the RGBA values of the pixel.

The fragment shader often receives texture coordinates, which describe where to sample the provided texture to get the color value corresponding to the pixel we're calculating at the moment. The texture can be sampled using the builtin `textureSample` function. The sampler that should be used for sampling the texture is provided in the header is called `sampler_`

```wgsl
let color = textureSample(texture, sampler_, texture_coordinates)
```

For example see this simple fragment shader, which applies the negative effect:

```wgsl
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let color = textureSample(textures[0], sampler_, input.tex_coords);
    return vec4(vec3(1.0) - color, 1.0);
}
```

## API

### Header

Every user-provided shader should include the code below.

```wgsl
struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) tex_coords: vec2<f32>,
}

struct BaseShaderParameters {
    plane_id: i32,
    time: f32,
    output_resolution: vec2<u32>,
    texture_count: u32,
}

@group(0) @binding(0) var textures: binding_array<texture_2d<f32>, 16>;
@group(2) @binding(0) var sampler_: sampler;

var<push_constant> base_params: BaseShaderParameters;
```

### Custom parameters

You can define a custom WGSL struct and bind a value of this type as

```wgsl
@group(1) @binding(0) var<uniform> custom_name: CustomStruct;
```

This struct has to be provided when creating a node using the `shader_params` field of the [shader node struct](./../api/components/Shader.md#shader).

### Entrypoints

The vertex shader entrypoint has to have the following signature:

```wgsl
@vertex
fn vs_main(input: VertexInput) -> A
```

Where `A` can be any user-defined struct suitable for a vertex shader output.

The fragment shader entrypoint has to have the following signature:

```wgsl
@fragment
fn fs_main(input: A) -> @location(0) vec4<f32>
```

Where `A` is the output type of the vertex shader.

Shaders have to be registered before they can be used. ([TypeScript](../typescript/instance.md#register-shader), [HTTP](../api/routes.md#register-shader))
