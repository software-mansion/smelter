struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) tex_coords: vec2<f32>,
}

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

struct BaseShaderParameters {
    plane_id: i32,
    time: f32,
    output_resolution: vec2<u32>,
    texture_count: u32,
}

@group(0) @binding(0) var textures: binding_array<texture_2d<f32>, 16>;
@group(2) @binding(0) var sampler_: sampler;

var<push_constant> base_params: BaseShaderParameters;

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    // Return transparent frame in case of different input video count
    if (base_params.texture_count != 1u) {
        return vec4(0.0, 0.0, 0.0, 0.0);
    }

    let pi = 3.14159;
    let effect_radius = abs(sin(base_params.time) / 2.0);
    let effect_angle = 2.0 * pi * abs(sin(base_params.time) / 2.0);

    let center = vec2(0.5, 0.5);
    let uv = input.tex_coords - center;

    let len = length(uv);
    let angle = atan2(uv.y, uv.x) + effect_angle * smoothstep(effect_radius, 0.0, len);
    let coords = vec2(len * cos(angle), len * sin(angle)) + center;

    return textureSample(textures[0], sampler_, coords);
}

