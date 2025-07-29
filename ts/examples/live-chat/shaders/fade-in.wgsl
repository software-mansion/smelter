struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) tex_coords: vec2<f32>,
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
}

struct BaseShaderParameters {
    plane_id: i32,
    time: f32,
    output_resolution: vec2<u32>,
    texture_count: u32,
}

@group(0) @binding(0) var textures: binding_array<texture_2d<f32>, 16>;
@group(2) @binding(0) var sampler_: sampler;
@group(1) @binding(0) var<uniform> animation_state: AnimationState;

var<push_constant> base_params: BaseShaderParameters;

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;

    output.position = vec4f(input.position, 1.0);
    output.tex_coords = input.tex_coords;

    return output;
}

struct AnimationState {
    local_time: f32,
    duration: f32,
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    if (base_params.texture_count != 1u) {
        return vec4(0.0, 0.0, 0.0, 0.0);
    }

    var color = textureSample(textures[0], sampler_, input.tex_coords);
    color.a *= min(animation_state.local_time / animation_state.duration, 1.0);

    return color;
}
