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

// Parameters:
//  - amplitude_px: vertical offset amplitude in pixels
//  - wavelength_px: length of one wave cycle in pixels (along X)
//  - speed: radians per second added to the phase (can be negative)
//  - phase: base phase offset in radians
struct ShaderOptions {
    amplitude_px: f32,
    wavelength_px: f32,
    speed: f32,
    phase: f32,
};

@group(0) @binding(0) var textures: binding_array<texture_2d<f32>, 16>;
@group(1) @binding(0) var<uniform> shader_options: ShaderOptions;
@group(2) @binding(0) var sampler_: sampler;

var<push_constant> base_params: BaseShaderParameters;

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.position = vec4(input.position, 1.0);
    out.tex_coords = input.tex_coords;
    return out;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    if (base_params.texture_count != 1u) { return vec4(0.0); }

    let res = vec2<f32>(f32(base_params.output_resolution.x), f32(base_params.output_resolution.y));
    let uv = input.tex_coords;

    // Parameters and sanity clamps
    let amp_px = max(0.0, shader_options.amplitude_px);
    let lambda_px = max(1.0, shader_options.wavelength_px);
    let speed = shader_options.speed;
    let phase = shader_options.phase;

    // Convert amplitude in px to UV space
    let amp_uv = amp_px / res.y;
    // Spatial frequency in radians per pixel along X
    let k = 2.0 * 3.141592653589793 * (1.0 / lambda_px);
    // Phase accumulation over time
    let wt = speed * base_params.time + phase;

    // Sample position after vertical displacement based on X
    let offset_y = amp_uv * sin(k * (uv.x * res.x) + wt);
    let uv_distorted = vec2<f32>(uv.x, clamp(uv.y + offset_y, 0.0, 1.0));

    let c = textureSample(textures[0], sampler_, uv_distorted);
    return c;
}


