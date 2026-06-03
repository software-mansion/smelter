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

@group(0) @binding(0) var texture: texture_2d<f32>;
@group(1) @binding(0) var sampler_: sampler;

@fragment
fn fs_main_y(input: VertexOutput) -> @location(0) f32 {
    let color = textureSample(texture, sampler_, input.tex_coords);

    // YUV conversion from: https://en.wikipedia.org/w/index.php?title=YCbCr&section=8#ITU-R_BT.709_conversion
    // YUV values footroom needs to be added
    // Y plane
    let y = color.r * 0.2126 + color.g * 0.7152 + color.b * 0.0722;
    // (235 - 16) / (255 - 0) = (219 / 255) ~= .858
    return clamp((y * 0.85882352941) + (16.0/255.0), 0.0, 1.0);
}

@fragment
fn fs_main_uv(input: VertexOutput) -> @location(0) vec2<f32> {
    let color = textureSample(texture, sampler_, input.tex_coords);

    // YUV conversion from: https://en.wikipedia.org/w/index.php?title=YCbCr&section=8#ITU-R_BT.709_conversion
    // YUV values footroom needs to be added
    // UV planes are returned in range (-0.5, 0.5) and need to be moved to (0, 1)
    // U plane
    let u = color.r * -0.1146 + color.g * -0.3854 + color.b * 0.5;
    // V plane
    let v = color.r * 0.5 + color.g * -0.4542 + color.b * -0.0458;
    // (240 - 16) / (255 - 0) = (224 / 255) ~= .878
    return clamp(vec2(
        ((u + 0.5) * 0.87843137254) + (16.0/255.0),
        ((v + 0.5) * 0.87843137254) + (16.0/255.0),
    ), vec2(0.0, 0.0), vec2(1.0, 1.0));
}
