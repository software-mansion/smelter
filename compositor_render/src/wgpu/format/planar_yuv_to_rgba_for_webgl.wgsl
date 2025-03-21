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

@group(0) @binding(0) var y_texture: texture_2d<f32>;
@group(0) @binding(1) var u_texture: texture_2d<f32>;
@group(0) @binding(2) var v_texture: texture_2d<f32>;

@group(1) @binding(0) var sampler_: sampler;

struct PushConstantParams {
    // 0 - YUV420P
    // 1 - YUVJ420P
    pixel_format: u32,
}

var<push_constant> params: PushConstantParams;

fn srgb_to_linear(srgb: vec3<f32>) -> vec3<f32> {
    let cutoff = step(srgb, vec3(0.04045));
    let higher = pow((srgb + vec3<f32>(0.055))/vec3<f32>(1.055), vec3<f32>(2.4));
    let lower = srgb/vec3(12.92);

    return mix(higher, lower, cutoff);
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    var y = textureSample(y_texture, sampler_, input.tex_coords).x;
    var u = textureSample(u_texture, sampler_, input.tex_coords).x;
    var v = textureSample(v_texture, sampler_, input.tex_coords).x;

    if params.pixel_format == 1u {
        // (235 - 16) / (255 - 0) = (219 / 255) ~= .858
        y = y * 0.85882352941 + (16.0/255.0);
        // (240 - 16) / (255 - 0) = (224 / 255) ~= .878
        u = u * 0.87843137254 + (16.0/255.0);
        v = v * 0.87843137254 + (16.0/255.0);
    }

    let r = y + 1.40200 * (v - 128.0 / 255.0);
    let g = y - 0.34414 * (u - 128.0 / 255.0) - 0.71414 * (v - 128.0 / 255.0);
    let b = y + 1.77200 * (u - 128.0 / 255.0);

    let srgb = vec3<f32>(clamp(r, 0.0, 1.0), clamp(g, 0.0, 1.0), clamp(b, 0.0, 1.0));
    return vec4(srgb_to_linear(srgb), 1.0);
}
