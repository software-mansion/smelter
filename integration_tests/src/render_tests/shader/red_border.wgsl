/// Adds red border to input

/// Depends on base_params.output_resolution  

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) tex_coords: vec2<f32>,
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
}

@group(0) @binding(0) var textures: binding_array<texture_2d<f32>, 16>;
@group(2) @binding(0) var sampler_: sampler;

struct BaseShaderParameters {
    plane_id: i32,
    time: f32,
    output_resolution: vec2<u32>,
    texture_count: u32,
}

var<push_constant> base_params: BaseShaderParameters;

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;

    output.position = vec4(input.position.x, input.position.y, input.position.z, 1.0);
    output.tex_coords = input.tex_coords;

    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let sample = textureSample(textures[0], sampler_, input.tex_coords);
    let border_size = 50.0;

    // input.position is interpolated into ([0, output_width], [0, output_height], ..) range
    // by wgpu between vertex shader output and fragment shader input
    if (input.position.x > border_size && input.position.x < f32(base_params.output_resolution.x) - border_size) {
        if (input.position.y > border_size && input.position.y < f32(base_params.output_resolution.y) - border_size) {
            return sample;
        }
    }
    
    return vec4(1.0, 0.0, 0.0, 1.0);
}

