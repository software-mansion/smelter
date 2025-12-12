struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) tex_coords: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
};

struct BaseShaderParameters {
    plane_id: i32,
    time: f32,
    output_resolution: vec2<u32>,
    texture_count: u32,
};

struct ShaderOptions {
    target_r: f32,
    target_g: f32,
    target_b: f32,
    tolerance: f32,
};

@group(0) @binding(0)
var textures: binding_array<texture_2d<f32>, 16>;

@group(1) @binding(0)
var<uniform> shader_options: ShaderOptions;

@group(2) @binding(0)
var sampler_: sampler;

var<push_constant> base_params: BaseShaderParameters;

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.position = vec4<f32>(input.position, 1.0);
    out.tex_coords = input.tex_coords;
    return out;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    if (base_params.texture_count != 1u) {
        return vec4<f32>(0.0);
    }

    let c = textureSample(textures[0], sampler_, input.tex_coords);
    // Remove if color is within tolerance of the target (component-wise max distance)
    let diff = vec3<f32>(abs(c.r - shader_options.target_r),
                         abs(c.g - shader_options.target_g),
                         abs(c.b - shader_options.target_b));
    let max_diff = max(diff.x, max(diff.y, diff.z));
    if (max_diff <= shader_options.tolerance) {
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }

    return c;
}


