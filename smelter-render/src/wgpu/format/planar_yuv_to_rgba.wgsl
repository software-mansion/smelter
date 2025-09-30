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
    // 0 - pixel format without J (limited colorspace range)
    // 1 - pixel format with J (full colorspace)
    pixel_format: u32,
}

var<push_constant> params: PushConstantParams;

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    var y = textureSample(y_texture, sampler_, input.tex_coords).x;
    var u = textureSample(u_texture, sampler_, input.tex_coords).x;
    var v = textureSample(v_texture, sampler_, input.tex_coords).x;

    // YUV conversion from: https://en.wikipedia.org/w/index.php?title=YCbCr&section=8#ITU-R_BT.709_conversion
    // YUV values footroom needs to be removed for non J formats
    // UV planes are in range (0, 1), but equation expects (-0.5, 0.5)

    if params.pixel_format == 0u {
        // (235 - 16) / (255 - 0) = (219 / 255) ~= .858
        y = clamp((y - (16.0/255.0)) / 0.85882352941, 0.0, 1.0);
        // (240 - 16) / (255 - 0) = (224 / 255) ~= .878
        u = clamp((u - (16.0/255.0)) / 0.87843137254, 0.0, 1.0);
        v = clamp((v - (16.0/255.0)) / 0.87843137254, 0.0, 1.0);
    }

    let r = y + 1.5748 * (v - 0.5);
    let g = y - 0.1873 * (u - 0.5) - 0.4681 * (v - 0.5);
    let b = y + 1.8556 * (u - 0.5);

    return vec4<f32>(clamp(r, 0.0, 1.0), clamp(g, 0.0, 1.0), clamp(b, 0.0, 1.0), 1.0);
}
