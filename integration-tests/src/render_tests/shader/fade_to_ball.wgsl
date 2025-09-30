/// Fades input "into ball" progersively in time 

/// Depends on base_params.time

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

    output.position = vec4(input.position, 1.0);
    output.tex_coords = input.tex_coords;

    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let sample = textureSample(textures[0], sampler_, input.tex_coords);

    let roll_to_ball_time = 5.0;
    let circle_radius = base_params.time / roll_to_ball_time;
    let epsilon = 0.15;

    let center = vec2(0.5, 0.5);
    let len = length(input.tex_coords - center);

    let transparency = smoothstep(circle_radius + epsilon, circle_radius - epsilon, len);

    return sample * transparency;
}

